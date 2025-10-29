use std::net::UdpSocket;
use crate::{camera::Camera, rtp::{RtpSender, RtpReceiver}, frame_handler::{Encoder, Decoder}, client::Client};
use std::net::SocketAddr;
use super::error::ControllerError as Error;
use std::thread;
use chrono::prelude::*;

pub struct Controller {
    pub client: Arc<RwLock<Client>>,
    pub camera: Arc<Camera>,
    pub rtp_sender: Arc<RwLock<Option<RtpSender<UdpSocket>>>>,
    pub rtp_receiver: Arc<RwLock<Option<RtpReceiver<UdpSocket>>>>,
    pub encoder: Arc<Encoder>,
    pub decoder: Arc<Decoder>,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            client: Arc::new(RwLock::new(Client::new())),
            camera: Arc::new(Camera::new()),
            rtp_sender: Arc::new(RwLock::new(None)),
            rtp_receiver: Arc::new(RwLock::new(None)),
            encoder: Arc::new(Encoder::new().unwrap()),
            decoder: Arc::new(Decoder::new().unwrap()),
        }
    }

    pub fn start_call(mut self: &Arc<Self>) -> Result<(), Error> {
        let pair_opt =  self.client.read().unwrap().ice_agent.get_selected_pair();

        if let Some(pair) = pair_opt {
            self.generate_media_threads(pair)?
        }

        Ok(())
    }

    fn generate_media_threads(mut self: &Arc<Self>, pair: &CandidatePair) -> Result<(), Error>{
        let local_rtp: SocketAddr = format!("{}:{}", pair.local.address, pair.local.port).parse()?;
        let local_rtcp: SocketAddr = format!("{}:{}", pair.local.address, pair.local.port + 1).parse()?;

        let remote_rtp: SocketAddr = format!("{}:{}", pair.remote.address, pair.remote.port).parse()?;
        let remote_rtcp: SocketAddr = format!("{}:{}", pair.remote.address, pair.remote.port + 1).parse()?;

        let rtp_socket = UdpSocket::bind(local_rtp).map_err(|e|Error::BindingAddressError(e.to_string()))?;
        let rtcp_socket = UdpSocket::bind(local_rtcp).map_err(|e|Error::BindingAddressError(e.to_string()))?;

        rtp_socket.connect(remote_rtp).map_err(|e| Error::ConnectionSocketError(e.to_string()))?;
        rtcp_socket.connect(remote_rtcp).map_err(|e| Error::ConnectionSocketError(e.to_string()))?;

        let sender = RtpSender::new(
            rtp_socket.try_clone().map_err(|e| Error::CloningSocketError(e.to_string()))?,
            rtcp_socket.try_clone().map_err(|e| Error::CloningSocketError(e.to_string()))?,
            42,
        ).map_err(|e| "Error: {e}")?;

        let receiver = RtpReceiver::new(rtp_socket, rtcp_socket).map_err(|e| "Error: {e}")?;

       *self.rtp_sender.write().unwrap() = Some(sender);
        *self.rtp_receiver.write().unwrap() = Some(receiver);

        let rx = self.camera.start();
        let ctrl = Arc::clone(self);

        thread::spawn(move || {
            for fr in rx {
                let frame_id = fr.id;
                let chunks = ctrl.encoder.encode_frame(fr).unwrap();

                for (i, c) in chunks.iter().enumerate() {
                    let mut guard = self.rtp_sender.write().unwrap();
                    if let Some(sender) = guard.as_mut() {
                        sender.send(
                            c,
                            96,
                            Local::now().timestamp_millis() as u32,
                            frame_id,
                            i as u64,
                            chunks.len() as u16).unwrap();
                    }
                }
            }
        });
        Ok(())
    }

}



/*

use crate::ice::{Candidate, CandidatePair};
use nokhwa::Camera;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use openh264::formats::{RgbSliceU8, YUVBuffer, YUVSource};
use std::sync::mpsc::Sender;
use std::sync::{mpsc, Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use anyhow::Result;

const FRAME_RATE: u32 = 30;

impl Client {
    pub fn start_media_threads(&mut self, selected_pair: CandidatePair) -> Result<(), Error> {
        // Create and store RTP communicators
        let remote_addr: SocketAddr = format!(
            "{}:{}",
            selected_pair.remote.address, selected_pair.remote.port
        )
            .parse()
            .map_err(|_| Error::IceConnectionError("Invalid remote addr".into()))?;

        let sender = RtpSender::new(remote_addr, remote_addr, 12345) // SSRC 12345
            .map_err(|e| Error::IceConnectionError(e.to_string()))?;
        let receiver = RtpReceiver::new(selected_pair.local.port) // Bind to our local port
            .map_err(|e| Error::IceConnectionError(e.to_string()))?;

        self.rtp_sender = Some(Arc::new(Mutex::new(sender)));
        self.rtp_receiver = Some(Arc::new(Mutex::new(receiver)));

        // Spawn the sending (camera) thread
        let sender_clone = self.rtp_sender.as_ref().unwrap().clone();
        let local_gui_sender = self.local_video_sender.clone();
        std::thread::spawn(move || {
            if let Err(e) = Self::run_sending_pipeline(sender_clone, local_gui_sender) {
                eprintln!("Sending pipeline error: {}", e);
            }
        });

        // Spawn the receiving (decoder) thread
        let receiver_clone = self.rtp_receiver.as_ref().unwrap().clone();
        let remote_gui_sender = self.remote_video_sender.clone();
        std::thread::spawn(move || {
            if let Err(e) = Self::run_receiving_pipeline(receiver_clone, remote_gui_sender) {
                eprintln!("Receiving pipeline error: {}", e);
            }
        });

        Ok(())
    }

    fn run_sending_pipeline(
        rtp_sender: Arc<Mutex<RtpSender>>,
        gui_sender: Sender<VideoFrame>,
    ) -> Result<()> {
        // Init Camera
        let mut camera = Camera::new(
            CameraIndex::Index(0),
            RequestedFormat::new::<nokhwa::pixel_format::RgbFormat>(
                RequestedFormatType::AbsoluteHighestFrameRate,
            ),
        )
            .unwrap();
        camera.open_stream().unwrap();

        // Init H264 Encoder
        let mut encoder = Encoder::new().unwrap();

        let frame_duration = Duration::from_millis(1000 / FRAME_RATE as u64);

        loop {
            let start_time = Instant::now();

            // Capture Frame
            let frame = camera.frame().unwrap();
            let resolution = frame.resolution();
            let width = resolution.width_x;
            let height = resolution.height_y;
            let rgb_data = frame.buffer().to_vec();

            let gui_frame = VideoFrame {
                rgb_data: frame.buffer().to_vec(), // Nokhwa gives RGB by default from MJPEG
                width,
                height,
            };
            let _ = gui_sender.send(gui_frame); // Ignore error if GUI closed

            // Encode Frame
            let rgb_source = RgbSliceU8::new(&rgb_data, (width as usize, height as usize));

            let yuv_buffer = YUVBuffer::from_rgb_source(rgb_source);

            // Encode the YUV buffer
            let bitstream = encoder.encode(&yuv_buffer)?;
            let h264_nal = bitstream.to_vec();

            // Packetize & Send (Simple Approach)
            let timestamp = start_time.elapsed().as_millis() as u32;

            let mut sender = rtp_sender.lock().unwrap();
            sender.send(&h264_nal, 96, timestamp, true)?;

            // Sleep to maintain framerate
            let elapsed = start_time.elapsed();
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
        }
    }

    fn run_receiving_pipeline(
        rtp_receiver: Arc<Mutex<RtpReceiver>>,
        gui_sender: Sender<VideoFrame>,
    ) -> Result<()> {
        // Init H264 Decoder
        let mut decoder = Decoder::new()?;

        loop {
            // Receive Packet
            let mut receiver = rtp_receiver.lock().unwrap();
            match receiver.try_receive()? {
                Some(rtp_package) => {
                    // De-packetize (Simple Approach)
                    let h264_nal = rtp_package.payload;

                    // Decode
                    if let Some(yuv_frame) = decoder.decode(&h264_nal)? {
                        // Convert YUV back to RGB for EGUI
                        // TODO: Implement yuv_to_rgb(yuv_frame)
                        let (width, height) = yuv_frame.dimensions();

                        let rgb_data = vec![128; (width * height * 3) as usize]; // Placeholder gray frame

                        let gui_frame = VideoFrame {
                            rgb_data,
                            width: width as u32,
                            height: height as u32,
                        };

                        // Send to GUI
                        if gui_sender.send(gui_frame).is_err() {
                            break; // GUI closed, exit thread
                        }
                    }
                }
                None => {
                    // No packet, sleep briefly to prevent 100% CPU
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }

        Ok(())
    }
}
 */