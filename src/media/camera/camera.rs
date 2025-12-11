use crate::config::Config;
use crate::media::camera::CameraError as Error;
use crate::media::frame_handler::Frame;
use opencv::{imgproc, prelude::*, videoio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::thread;
use crate::clock::Clock;

/// Trait for objects that produce frames from a video source.
///
/// Implementers of this trait are responsible for capturing frames from
/// a media source and delivering them through a channel.
pub trait FrameSource: Send {
    /// Starts the capture thread.
    ///
    /// Spawns a background thread that continuously captures frames from the source
    /// and sends them through the returned receiver.
    ///
    /// # Returns
    ///
    /// * `Ok(Receiver<Frame>)` - Channel receiver for frame delivery.
    /// * `Err(Error)` - If the source cannot be started.
    fn start(&mut self) -> Result<Receiver<Frame>, Error>;

    /// Signals the capture thread to stop.
    ///
    /// Gracefully stops frame capture and terminates the background thread.
    fn stop(&self);
}

/// A camera device that captures frames and produces them through a channel.
///
/// `Camera` is a wrapper around OpenCV's `VideoCapture` that provides convenient
/// frame capture with thread safety. It manages a background capture thread that
/// reads frames from the camera device, converts them to RGB format, and sends
/// them through a channel.
///
/// # Fields
///
/// * `running` - Atomic flag to control the capture thread lifecycle.
/// * `config` - Application configuration containing camera settings.
/// * `clock` - Clock instance for timestamping captured frames.
pub struct Camera {
    /// Flag used to signal the capture thread to keep running.
    running: Arc<AtomicBool>,

    /// Application configuration with camera parameters.
    config: Arc<Config>,
    
    /// Clock for frame timestamping.
    clock: Arc<Clock>
}

impl Camera {
    /// Creates a new camera instance with the provided configuration.
    ///
    /// Initializes the camera with the given clock and media configuration,
    /// but does not start capturing frames until `start()` is called.
    ///
    /// # Parameters
    ///
    /// * `clock` - Clock instance for frame timestamping.
    /// * `media_config` - Application configuration with camera index, frame size, and frame rate.
    ///
    /// # Returns
    ///
    /// A new `Camera` instance ready to be started.
    #[must_use]
    pub fn new(clock: Arc<Clock>, media_config: &Arc<Config>) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            config: Arc::clone(media_config),
            clock
        }
    }

    /// Starts the capture thread and returns a frame receiver.
    ///
    /// Spawns a background thread that continuously captures frames from the camera device,
    /// converts them from BGR to RGB format, and sends them through the returned channel.
    /// Frames are captured at the configured frame rate and resolution.
    ///
    /// # Returns
    ///
    /// * `Ok(Receiver<Frame>)` - Channel receiver for captured frames.
    /// * `Err(Error)` - If the camera cannot be opened, configured, or capture fails.
    fn start_internal(&mut self) -> Result<Receiver<Frame>, Error> {
        let (tx, rx) = mpsc::channel();
        let running = self.running.clone();
        let config = self.config.clone();
        let clock = self.clock.clone();

        running.store(true, SeqCst);

        let camera_index = if let Ok(index) = i32::try_from(config.media.camera_index) {
            index
        } else {
            return Err(Error::IndexError);
        };

        let mut cam = match videoio::VideoCapture::new(camera_index, videoio::CAP_FFMPEG) {
            Ok(cam) => cam,
            Err(e) => {
                return Err(Error::OpenError(e.to_string()))
            }
        };
        if !videoio::VideoCapture::is_opened(&cam).map_err(|_| Error::ClosedCamera)? {
            return Err(Error::ClosedCamera)
        }


        if cam.set(videoio::CAP_PROP_FRAME_WIDTH, config.media.frame_width).is_err()
            || cam.set(videoio::CAP_PROP_FRAME_HEIGHT, config.media.frame_height).is_err()
            ||cam.set(videoio::CAP_PROP_FPS, config.media.frame_rate as f64).is_err()
        {
            return Err(Error::CameraConfigFailed)
        }
        
        let mut mat = Mat::default();
        let mut rgb = Mat::default();
        
        thread::spawn(move || {
            while running.load(SeqCst) {
                if !cam.read(&mut mat).unwrap_or(false) || mat.empty() {
                    eprintln!("Failed to read camera frame.");
                    continue;
                }

                if imgproc::cvt_color(
                    &mat,
                    &mut rgb,
                    imgproc::COLOR_BGR2RGB,
                    0,
                )
                .is_err()
                {
                    eprintln!("Failed to convert frame color.");
                    continue;
                }
                let data = if let Ok(bytes) = rgb.data_bytes() {
                    bytes.to_vec()
                } else {
                    eprintln!("Failed to extract frame data.");
                    continue;
                };

                #[allow(clippy::cast_sign_loss)]
                let frame = Frame {
                    data,
                    width: rgb.cols() as usize,
                    height: rgb.rows() as usize,
                    frame_time: clock.now()
                };

                if tx.send(frame).is_err() {
                    continue;
                }
            }
        });
        Ok(rx)
    }

    /// Stops the capture thread.
    ///
    /// Clears the running flag, which signals the background capture thread to exit gracefully.
    /// The thread will observe this flag and stop capturing frames.
    fn stop_internal(&self) {
        self.running.store(false, SeqCst);
    }
}

impl FrameSource for Camera {
    /// Starts the camera frame capture.
    ///
    /// Delegates to the internal start implementation.
    fn start(&mut self) -> Result<Receiver<Frame>, Error> {
        self.start_internal()
    }

    /// Stops the camera frame capture.
    ///
    /// Delegates to the internal stop implementation.
    fn stop(&self) {
        self.stop_internal()
    }
}
