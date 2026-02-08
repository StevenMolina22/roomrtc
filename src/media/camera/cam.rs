use crate::clock::Clock;
use crate::config::Config;
use crate::logger::Logger;
use crate::media::camera::CameraError as Error;
use crate::media::frame_handler::Frame;
use opencv::videoio::VideoWriter;
use opencv::{imgproc, prelude::*, videoio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub trait FrameSource: Send {
    /// Start the capture thread.
    /// Returns a Receiver to get frames from.
    fn start(&mut self) -> Receiver<Frame>;

    /// Signal the capture thread to stop.
    fn stop(&self);
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
    running: Arc<AtomicBool>,

    /// Config file
    config: Arc<Config>,

    /// Clock of the program
    clock: Arc<Clock>,

    logger: Logger,
}

impl Camera {
    /// Create a new `Camera` configured with `media_config`.
    ///
    /// # Parameters
    /// - `media_config`: media capture configuration (camera index,
    ///   frame size and frame rate).
    #[must_use]
    pub fn new(clock: Arc<Clock>, media_config: &Arc<Config>, logger: Logger) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            config: Arc::clone(media_config),
            clock,
            logger,
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
    fn start_internal(&mut self) -> Receiver<Frame> {
        let (tx, rx) = mpsc::channel();
        let running = self.running.clone();
        let config = self.config.clone();
        let clock = self.clock.clone();
        let logger = self.logger.clone();

        running.store(true, Ordering::SeqCst);

        let cam = match setup_camera(&config) {
            Ok(cam) => cam,
            Err(e) => {
                logger.error(&e.to_string());
                return rx;
            }
        };
        thread::spawn(move || {
            run_camera_loop(cam, running, tx, clock, logger);
        });

        rx
    }

    /// Stop the capture thread by clearing the running flag. The
    /// capture thread will observe this and exit shortly.
    fn stop_internal(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

impl FrameSource for Camera {
    fn start(&mut self) -> Receiver<Frame> {
        self.start_internal()
    }

    fn stop(&self) {
        self.stop_internal()
    }
}

// Aux functions ----------------------------------------------------------------------------------

fn setup_camera(config: &Arc<Config>) -> Result<videoio::VideoCapture, Error> {
    let camera_index =
        i32::try_from(config.media.camera_index).map_err(|_| Error::CameraIndexError)?;

    let mut cam = videoio::VideoCapture::new(camera_index, videoio::CAP_V4L2)
        .map_err(|e| Error::OpenError(e.to_string()))?;

    if !videoio::VideoCapture::is_opened(&cam).unwrap_or(false) {
        return Err(Error::OpenError("Camera not opened".to_string()));
    }

    let settings = [
        (
            videoio::CAP_PROP_FOURCC,
            f64::from(VideoWriter::fourcc('M', 'J', 'P', 'G').unwrap()),
        ),
        (videoio::CAP_PROP_FPS, config.media.frame_rate as f64),
        (videoio::CAP_PROP_FRAME_WIDTH, config.media.frame_width),
        (videoio::CAP_PROP_FRAME_HEIGHT, config.media.frame_height),
    ];

    for (prop, val) in settings {
        cam.set(prop, val)
            .map_err(|_| Error::PropSettingError(prop.to_string()))?;
    }

    Ok(cam)
}

fn run_camera_loop(
    mut cam: videoio::VideoCapture,
    running: Arc<AtomicBool>,
    tx: Sender<Frame>,
    clock: Arc<Clock>,
    logger: Logger,
) {
    let mut mat = Mat::default();
    let mut rgb = Mat::default();

    while running.load(Ordering::SeqCst) {
        if !cam.read(&mut mat).unwrap_or(false) || mat.empty() {
            logger.error(&Error::ReadFrameError.to_string());
            continue;
        }

        if imgproc::cvt_color(&mat, &mut rgb, imgproc::COLOR_BGR2RGB, 0).is_err() {
            continue;
        }

        if let Ok(data) = rgb.data_bytes() {
            let frame = Frame {
                data: data.to_vec(),
                width: rgb.cols() as usize,
                height: rgb.rows() as usize,
                frame_time: clock.now(),
            };
            if tx.send(frame).is_err() {
                break;
            }
        }
    }
}
