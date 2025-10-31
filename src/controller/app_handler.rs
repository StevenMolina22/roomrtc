use std::net::UdpSocket;
use crate::{camera::Camera, rtp::{RtpSender, RtpReceiver}, frame_handler::{Encoder, Decoder}, client::Client};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::{channel, Receiver, Sender};
use super::error::ControllerError as Error;
use std::thread;
use std::thread::JoinHandle;
use chrono::prelude::*;
use crate::ice::CandidatePair;
use crate::frame_handler::{Frame, EncodedFrame};
use crate::rtp::{ConnectionStatus, RtpPacket};

pub struct Controller {
    pub client: Client,
    pub tx_encoded: Sender<EncodedFrame>,
    pub rx_encoded: Arc<Mutex<Receiver<EncodedFrame>>>,
    pub tx_local: Sender<Frame>,
    pub tx_remote: Sender<Frame>,
    pub connection_status: Arc<RwLock<ConnectionStatus>>,
    pub camera_handler: Option<JoinHandle<()>>,
    pub rtp_sender_handler: Option<JoinHandle<()>>,
    pub rtp_receiver_handler: Option<JoinHandle<()>>,
}

impl Controller {
    pub fn new(tx_local: Sender<Frame>, tx_remote: Sender<Frame>) -> Self {
        let (tx_encoded, rx_encoded) = channel();

        Self {
            client: Client::new(),
            tx_encoded,
            rx_encoded: Arc::new(Mutex::new(rx_encoded)),
            tx_local,
            tx_remote,
            connection_status: Arc::new(RwLock::new(ConnectionStatus::Closed)),
            camera_handler: None,
            rtp_sender_handler: None,
            rtp_receiver_handler: None,
        }
    }

    pub fn start_call(&mut self) -> Result<(), Error> {
        {
            let mut conn = self.connection_status.write().map_err(|_| Error::PoisonedLock)?;
            *conn = ConnectionStatus::Open;
        }
        
        let pair_opt = {
            let client_ref = &self.client;
            client_ref.ice_agent.get_selected_pair().cloned()
        };

        if let Some(pair) = pair_opt {
            self.generate_media_threads(&pair)?;
        }

        Ok(())
    }

    pub fn shut_down(&mut self) -> Result<(), Error> {
        let mut conn = self.connection_status.write().map_err(|_| Error::PoisonedLock)?;
        *conn = ConnectionStatus::Closed;
        
        Ok(())
    }


    fn generate_media_threads(&mut self, pair: &CandidatePair) -> Result<(), Error> {
        let local_rtp: SocketAddr = format!("{}:{}", pair.local.address, pair.local.port)
            .parse()
            .map_err(|e| Error::ParsingSocketAddressError(e))?;
        let local_rtcp: SocketAddr = format!("{}:{}", pair.local.address, pair.local.port + 1)
            .parse()
            .map_err(|e| Error::ParsingSocketAddressError(e))?;

        let remote_rtp: SocketAddr = format!("{}:{}", pair.remote.address, pair.remote.port)
            .parse()
            .map_err(|e| Error::ParsingSocketAddressError(e))?;
        let remote_rtcp: SocketAddr = format!("{}:{}", pair.remote.address, pair.remote.port + 1)
            .parse()
            .map_err(|e| Error::ParsingSocketAddressError(e))?;


        let rtp_sender_socket = UdpSocket::bind(local_rtp)
            .map_err(|e| Error::BindingAddressError(e.to_string()))?;
        let rtcp_sender_socket = UdpSocket::bind(local_rtcp)
            .map_err(|e| Error::BindingAddressError(e.to_string()))?;

        rtp_sender_socket
            .connect(remote_rtp)
            .map_err(|e| Error::ConnectionSocketError(e.to_string()))?;
        rtcp_sender_socket
            .connect(remote_rtcp)
            .map_err(|e| Error::ConnectionSocketError(e.to_string()))?;

        let rtp_receiver_socket = rtp_sender_socket.try_clone().map_err(|e| Error::CloningSocketError(e.to_string()))?;
        let rtcp_receiver_socket = rtcp_sender_socket.try_clone().map_err(|e| Error::CloningSocketError(e.to_string()))?;

        self.spawn_camera_thread();
        self.spawn_rtp_sender_thread(rtp_sender_socket, rtcp_sender_socket, Arc::clone(&self.connection_status))?;
        self.spawn_rtp_receiver_thread(rtp_receiver_socket, rtcp_receiver_socket, Arc::clone(&self.connection_status));

        Ok(())
    }

    pub fn spawn_camera_thread(&mut self) {
        let tx_local_cam = self.tx_local.clone();
        let tx_encoded = self.tx_encoded.clone();

        let handler = thread::spawn({
            let mut camera = Camera::new();
            let rx_camera = camera.start();
            let mut encoder = Encoder::new().unwrap();
            move || {
                for frame in rx_camera {
                    if tx_local_cam.send(frame.clone()).is_err() {
                        break;
                    }
                    let encoded = encoder.encode_frame(&frame).unwrap();
                    let encoded_frame = EncodedFrame {
                        id: frame.id,
                        chunks: encoded,
                        width: frame.width,
                        height: frame.height,
                    };

                    tx_encoded.send(encoded_frame).unwrap();
                }
            }
        });
        self.camera_handler = Some(handler);
    }

    fn spawn_rtp_sender_thread(
        &mut self,
        rtp_socket: UdpSocket,
        rtcp_socket: UdpSocket,
        connection_status: Arc<RwLock<ConnectionStatus>>,
    ) -> Result<(), Error> {
        let rx_encoded = self.rx_encoded.clone();

        thread::spawn({
            let mut rtp_sender = RtpSender::new(
                rtp_socket
                    .try_clone()
                    .map_err(|e| Error::CloningSocketError(e.to_string()))?,
                rtcp_socket
                    .try_clone()
                    .map_err(|e| Error::CloningSocketError(e.to_string()))?,
                42,
                connection_status,
            )
                .map_err(|e| Error::RtpSenderError(e.to_string()))?;
            move || {
                loop {
                    let encoded_frame = rx_encoded.lock().unwrap().recv().unwrap();
                    println!("received frame from encoder thread");
                    for (i, c) in encoded_frame.chunks.iter().enumerate() {
                        rtp_sender
                            .send(
                                c,
                                96,
                                Local::now().timestamp_millis() as u32,
                                encoded_frame.id,
                                i as u64,
                                encoded_frame.chunks.len() as u16,
                            )
                            .unwrap();
                    }
                }
            }
        });
        Ok(())
    }

    fn spawn_rtp_receiver_thread(&self, rtp_receiver_socket: UdpSocket, rtcp_receiver_socket: UdpSocket, connection_status: Arc<RwLock<ConnectionStatus>>,) {
        let tx_remote_cam_receiver = self.tx_remote.clone();

        thread::spawn({
            let mut receiver = RtpReceiver::new(rtp_receiver_socket, rtcp_receiver_socket, connection_status).map_err(|e| Error::RtpReceiverError(e.to_string())).unwrap();
            let mut decoder = Decoder::new().unwrap();
            move || {
                let mut actual_frame = None;
                let mut chunks = Vec::new();

                loop {
                    let rtp_packet = receiver.receive().map_err(|e| Error::RtpReceiverError(e.to_string())).unwrap();
                    match actual_frame {
                        Some(act_frame_id) => {
                            if act_frame_id == rtp_packet.frame_id {
                                chunks.push(rtp_packet.clone());
                            } else {
                                chunks = vec![rtp_packet.clone()];
                                actual_frame = Some(rtp_packet.frame_id);
                            }

                            if rtp_packet.marker == chunks.len() as u16 {
                                let frame_data = generate_frame_from(&mut chunks, &mut decoder);
                                tx_remote_cam_receiver.send(frame_data)
                                    .map_err(|e| Error::RtpSenderError(e.to_string()))
                                    .unwrap();
                            }
                        },
                        None => {
                            actual_frame = Some(rtp_packet.frame_id);
                            chunks.push(rtp_packet.clone());
                        }
                    }
                }
            }
        });
    }
}
fn generate_frame_from(chunks: &mut Vec<RtpPacket>, decoder: &mut Decoder) -> Frame {
    let fr_id = chunks.first().unwrap().frame_id;
    chunks.sort_by_key(|c| c.chunk_id);
    let mut data = Vec::new();
    for c in chunks.iter() {
        data.extend_from_slice(&c.payload);
    }
    let (decoded_data, width, height) =decoder.decode_frame(&data).unwrap();
    
    Frame {
        data: decoded_data,
        width,
        height,
        id: fr_id,
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