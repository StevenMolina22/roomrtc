use crate::config::MediaConfig;
use crate::frame_handler::Frame;
use opencv::{imgproc, prelude::*, videoio, core};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

pub struct Camera {
    running: Arc<RwLock<bool>>,
    frame_id: Arc<RwLock<usize>>,
    media_config: MediaConfig,
}

impl Camera {
    pub fn new(media_config: MediaConfig) -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
            frame_id: Arc::new(RwLock::new(0)),
            media_config,
        }
    }

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
                        eprintln!("Failed to open camera: {}. Stopping camera thread.", e);
                        return; // Exit thread
                    }
                };

            if !videoio::VideoCapture::is_opened(&cam).unwrap_or(false) {
                eprintln!("Camera is not open. Stopping camera thread.");
                return; // Exit thread
            }
            cam.set(videoio::CAP_PROP_FRAME_WIDTH, config.frame_width)
                .unwrap();
            cam.set(videoio::CAP_PROP_FRAME_HEIGHT, config.frame_height)
                .unwrap();

            let mut mat = Mat::default();
            let mut rgb = Mat::default();

            let frame_duration = Duration::from_millis(1000 / config.frame_rate as u64);

            while *running.read().unwrap() {
                if !cam.read(&mut mat).unwrap() || mat.empty() {
                    continue;
                }

                imgproc::cvt_color(&mat, &mut rgb, imgproc::COLOR_BGR2RGB, 0, core::AlgorithmHint::ALGO_HINT_DEFAULT).unwrap();
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

    pub fn stop(&self) {
        let mut run = self.running.write().unwrap();
        *run = false;
    }
}
