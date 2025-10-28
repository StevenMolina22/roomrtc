use std::io::Read;
use opencv::{
    prelude::*,
    videoio,
    imgproc,
    core
};
use std::sync::{Arc, RwLock};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

pub struct Frame {
    data: Vec<u8>,
    width: u32,
    height: u32
}
pub struct Camera {
    running: Arc<RwLock<bool>>,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub fn start(&self) -> Receiver<Frame> {
        let (tx, rx) = mpsc::channel();
        let running = self.running.clone();
        *running.write().unwrap() = true;

        thread::spawn(move || {
            let mut cam = videoio::VideoCapture::new(0, videoio::CAP_ANY).unwrap();
            if !videoio::VideoCapture::is_opened(&cam).unwrap() {
                return
            }
            cam.set(videoio::CAP_PROP_FRAME_WIDTH, 640.0).unwrap();
            cam.set(videoio::CAP_PROP_FRAME_HEIGHT, 480.0).unwrap();

            let mut mat = Mat::default();
            let mut yuv = Mat::default();

            while *running.read().unwrap() {
                if !cam.read(&mut mat).unwrap() || mat.empty() {
                    continue;
                }

                imgproc::cvt_color(&mat, &mut yuv, imgproc::COLOR_BGR2YUV_I420, 0, core::AlgorithmHint::ALGO_HINT_DEFAULT).unwrap();
                let data = yuv.data_bytes().unwrap().to_vec();

                let frame = Frame {
                    data,
                    width: yuv.cols() as u32,
                    height: yuv.rows() as u32,
                };

                if tx.send(frame).is_err() {
                    break;
                }
                
                thread::sleep(Duration::from_millis(10));
            }

        });

        rx
    }

    pub fn stop(&self) {
        let mut run = self.running.write().unwrap();
        *run = false;
    }
}
