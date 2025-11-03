use crate::config::MediaConfig;
use crate::frame_handler::Frame;
use opencv::{imgproc, prelude::*, videoio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

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
    media_config: MediaConfig,
}

impl Camera {
    /// Create a new `Camera` configured with `media_config`.
    ///
    /// # Parameters
    /// - `media_config`: media capture configuration (camera index,
    ///   frame size and frame rate).
    #[must_use]
    pub fn new(media_config: MediaConfig) -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            frame_id: Arc::new(RwLock::new(0)),
            media_config,
        }
    }

    /// Start the capture thread and return a channel `Receiver<Frame>`
    /// where captured frames will be sent.
    ///
    /// The returned receiver receives `Frame` instances continuously
    /// until `stop()` is called or the sender is dropped. This method
    /// spawns a background thread that captures frames from `OpenCV`'s
    /// `VideoCapture` and converts them to RGB `Frame`s at the
    /// configured frame rate.
    pub fn start(&mut self) -> Receiver<Frame> {
        let (tx, rx) = mpsc::channel();
        let running = self.running.clone();
        *running.write().unwrap() = true;
        let frame_id = self.frame_id.clone();

        let config = self.media_config.clone();

        thread::spawn(move || {
            let mut cam =
                match videoio::VideoCapture::new(config.camera_index as i32, videoio::CAP_ANY) {
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
            cam.set(videoio::CAP_PROP_FRAME_WIDTH, config.frame_width)
                .unwrap();
            cam.set(videoio::CAP_PROP_FRAME_HEIGHT, config.frame_height)
                .unwrap();

            let mut mat = Mat::default();
            let mut rgb = Mat::default();

            let frame_duration = Duration::from_millis(1000 / u64::from(config.frame_rate));

            while *running.read().unwrap() {
                if !cam.read(&mut mat).unwrap() || mat.empty() {
                    continue;
                }

                imgproc::cvt_color(&mat, &mut rgb, imgproc::COLOR_BGR2RGB, 0).unwrap();
                let data = rgb.data_bytes().unwrap().to_vec();

                let id = {
                    let mut id_lock = frame_id.write().unwrap();
                    let id = *id_lock;
                    *id_lock = id + 1;
                    id
                };

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

        rx
    }

    /// Stop the capture thread by clearing the running flag. The
    /// capture thread will observe this and exit shortly.
    pub fn stop(&self) {
        let mut run = self.running.write().unwrap();
        *run = false;
    }
}
