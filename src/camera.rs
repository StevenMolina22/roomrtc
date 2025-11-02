use crate::frame_handler::Frame;
use opencv::{imgproc, prelude::*, videoio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

/// A camera that runs a capture thread and produces
/// `Frame` instances over a channel.
///
/// `Camera` is a convenience wrapper around OpenCV's `VideoCapture`.
/// It exposes `start`/`stop` operations and internally uses shared
/// state (`Arc<RwLock<...>>`) to control the capture thread and to
/// produce incremental frame ids.
pub struct Camera {
    /// Flag used to signal the capture thread to keep running.
    running: Arc<RwLock<bool>>,

    /// Monotonic counter used to assign `Frame.id` values.
    frame_id: Arc<RwLock<usize>>,
}

impl Camera {
    /// Create a new `Camera`. The capture thread is not
    /// started automatically; call `start()` to begin producing
    /// frames.
    pub fn new() -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            frame_id: Arc::new(RwLock::new(0)),
        }
    }

    pub fn start(&mut self) -> Receiver<Frame> {
        let (tx, rx) = mpsc::channel();
        let running = self.running.clone();
        *running.write().unwrap() = true;
        let frame_id = self.frame_id.clone();

        thread::spawn(move || {
            let mut cam = match videoio::VideoCapture::new(0, videoio::CAP_ANY) {
                Ok(cam) => cam,
                Err(e) => {
                    eprintln!("Failed to open camera: {}. Stopping camera thread.", e);
                    return; // Exit thread
                }
            };

            if !videoio::VideoCapture::is_opened(&cam).unwrap_or(false) {
                eprintln!("Camera is not open. Stopping camera thread.");
                return; // Exit thread
            }
            cam.set(videoio::CAP_PROP_FRAME_WIDTH, 640.0).unwrap();
            cam.set(videoio::CAP_PROP_FRAME_HEIGHT, 480.0).unwrap();

            let mut mat = Mat::default();
            let mut rgb = Mat::default();

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

                thread::sleep(Duration::from_millis(33));
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
