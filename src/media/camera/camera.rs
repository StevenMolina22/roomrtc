use crate::config::Config;
use crate::media::camera::CameraError as Error;
use crate::media::frame_handler::Frame;
use opencv::{imgproc, prelude::*, videoio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

pub trait FrameSource: Send {
    /// Start the capture thread.
    /// Returns a Receiver to get frames from.
    fn start(&mut self) -> Result<Receiver<Frame>, Error>;

    /// Signal the capture thread to stop.
    fn stop(&self) -> Result<(), Error>;
}

/// A camera that runs a capture thread and produces
/// `Frame` instances over a channel.
///
/// `Camera` is a convenience wrapper around `OpenCV`'s `VideoCapture`.
/// It exposes `start`/`stop` operations and internally uses shared
/// state (`Arc<RwLock<...>>`) to control the capture thread and to
/// produce incremental frame ids.
pub struct Camera {
    /// Flag used to signal the capture thread to keep running.
    running: Arc<RwLock<bool>>,

    /// Monotonic counter used to assign `Frame.id` values.
    frame_id: Arc<RwLock<usize>>,
    config: Arc<Config>,
}

impl Camera {
    /// Create a new `Camera` configured with `media_config`.
    ///
    /// # Parameters
    /// - `media_config`: media capture configuration (camera index,
    ///   frame size and frame rate).
    #[must_use]
    pub fn new(media_config: &Arc<Config>) -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            frame_id: Arc::new(RwLock::new(0)),
            config: Arc::clone(media_config),
        }
    }
}

impl Camera {
    /// Start the capture thread and return a channel `Receiver<Frame>`
    /// where captured frames will be sent.
    ///
    /// The returned receiver receives `Frame` instances continuously
    /// until `stop()` is called or the sender is dropped. This method
    /// spawns a background thread that captures frames from `OpenCV`'s
    /// `VideoCapture` and converts them to RGB `Frame`s at the
    /// configured frame rate.
    fn start_internal(&mut self) -> Result<Receiver<Frame>, Error> {
        let (tx, rx) = mpsc::channel();
        let running = self.running.clone();
        *running.write().map_err(|_| Error::PoisonedLock)? = true;
        let frame_id = self.frame_id.clone();

        let config = self.config.clone();

        thread::spawn(move || {
            let camera_index = if let Ok(index) = i32::try_from(config.media.camera_index) {
                index
            } else {
                eprintln!("Camera index is too large for i32. Stopping camera thread.");
                return;
            };

            let mut cam = match videoio::VideoCapture::new(camera_index, videoio::CAP_ANY) {
                Ok(cam) => cam,
                Err(e) => {
                    eprintln!("Failed to open camera: {e}. Stopping camera thread.");
                    return;
                }
            };

            if !videoio::VideoCapture::is_opened(&cam).unwrap_or(false) {
                eprintln!("Camera is not open. Stopping camera thread.");
                return;
            }
            if cam
                .set(videoio::CAP_PROP_FRAME_WIDTH, config.media.frame_width)
                .is_err()
            {
                eprintln!("Failed to set camera width. Stopping camera thread.");
                return;
            }
            if cam
                .set(videoio::CAP_PROP_FRAME_HEIGHT, config.media.frame_height)
                .is_err()
            {
                eprintln!("Failed to set camera height. Stopping camera thread.");
                return;
            }

            let mut mat = Mat::default();
            let mut rgb = Mat::default();

            let frame_duration = Duration::from_millis(1000 / u64::from(config.media.frame_rate));

            while {
                match running.read() {
                    Ok(guard) => *guard,
                    Err(e) => {
                        eprintln!("Failed to read running flag: {e}");
                        false
                    }
                }
            } {
                if !cam.read(&mut mat).unwrap_or(false) || mat.empty() {
                    eprintln!("Failed to read camera frame.");
                    continue;
                }

                if imgproc::cvt_color(&mat, &mut rgb, imgproc::COLOR_BGR2RGB, 0).is_err() {
                    eprintln!("Failed to convert frame color.");
                    continue;
                }
                let data = if let Ok(bytes) = rgb.data_bytes() {
                    bytes.to_vec()
                } else {
                    eprintln!("Failed to extract frame data.");
                    continue;
                };

                let id = {
                    let mut id_lock = if let Ok(lock) = frame_id.write() {
                        lock
                    } else {
                        eprintln!("Failed to acquire frame ID lock.");
                        continue;
                    };
                    let id = *id_lock;
                    *id_lock = id + 1;
                    id
                };

                #[allow(clippy::cast_sign_loss)]
                let frame = Frame {
                    data,
                    width: rgb.cols() as usize,
                    height: rgb.rows() as usize,
                    id: id as u64,
                };

                if tx.send(frame).is_err() {
                    break;
                }

                thread::sleep(frame_duration);
            }
        });

        Ok(rx)
    }

    /// Stop the capture thread by clearing the running flag. The
    /// capture thread will observe this and exit shortly.
    fn stop_internal(&self) -> Result<(), Error> {
        let mut run = self.running.write().map_err(|_| Error::PoisonedLock)?;
        *run = false;
        Ok(())
    }
}

impl FrameSource for Camera {
    fn start(&mut self) -> Result<Receiver<Frame>, Error> {
        self.start_internal()
    }

    fn stop(&self) -> Result<(), Error> {
        self.stop_internal()
    }
}
