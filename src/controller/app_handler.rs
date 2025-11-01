use super::error::{ControllerError as Error, ThreadsError};
use crate::frame_handler::{EncodedFrame, Frame};
use crate::ice::CandidatePair;
use crate::rtp::{ConnectionStatus, RtpPacket};
use crate::{
    camera::Camera,
    client::Client,
    frame_handler::{Decoder, Encoder},
    rtp::{RtpReceiver, RtpSender},
};
use chrono::prelude::*;
use std::net::SocketAddr;
use std::net::UdpSocket;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::thread::JoinHandle;

pub struct Controller {
    pub client: Client,

    //Channels
    pub tx_encoded: Sender<EncodedFrame>,
    pub rx_encoded: Arc<Mutex<Receiver<EncodedFrame>>>,
    pub tx_local: Sender<Frame>,
    pub tx_remote: Sender<Frame>,
    pub tx_thread: Sender<ThreadsError>,
    pub rx_thread: Arc<Mutex<Receiver<ThreadsError>>>,

    //Connection status
    pub connection_status: Arc<RwLock<ConnectionStatus>>,

    //Camera
    pub camera: Arc<Mutex<Camera>>,

    //Thread's handlers
    pub camera_handler: Option<JoinHandle<()>>,
    pub rtp_sender_handler: Option<JoinHandle<()>>,
    pub rtp_receiver_handler: Option<JoinHandle<()>>,

    rtp_socket: UdpSocket,
    rtcp_socket: UdpSocket,
}

impl Controller {
    pub fn new(tx_local: Sender<Frame>, tx_remote: Sender<Frame>) -> Self {
        let (tx_encoded, rx_encoded) = channel();
        let (tx_thread, rx_thread) = channel();

        let rtp_socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind RTP socket");
        let rtp_port = rtp_socket.local_addr().unwrap().port();

        // 2. Bind RTCP socket to the *next* port
        let rtcp_addr = format!("0.0.0.0:{}", rtp_port + 1);
        let rtcp_socket = UdpSocket::bind(&rtcp_addr)
            .unwrap_or_else(|e| panic!("Failed to bind RTCP socket on {}: {}", rtcp_addr, e));

        Self {
            client: Client::new(rtp_port),
            tx_encoded,
            rx_encoded: Arc::new(Mutex::new(rx_encoded)),
            tx_local,
            tx_remote,
            tx_thread,
            rx_thread: Arc::new(Mutex::new(rx_thread)),
            connection_status: Arc::new(RwLock::new(ConnectionStatus::Closed)),
            camera: Arc::new(Mutex::new(Camera::new())),
            camera_handler: None,
            rtp_sender_handler: None,
            rtp_receiver_handler: None,
            rtp_socket,
            rtcp_socket,
        }
    }

    pub fn start_call(&mut self) -> Result<(), Error> {
        {
            let mut conn = self
                .connection_status
                .write()
                .map_err(|_| Error::PoisonedLock)?;
            *conn = ConnectionStatus::Open;
        }

        let pair_opt = {
            let client_ref = &self.client;
            client_ref.ice_agent.get_selected_pair().cloned()
        };

        if let Ok(pair) = pair_opt {
            self.generate_media_threads(&pair)?;
        }

        Ok(())
    }

    pub fn shut_down(&mut self) -> Result<(), Error> {
        let mut conn = self
            .connection_status
            .write()
            .map_err(|_| Error::PoisonedLock)?;
        *conn = ConnectionStatus::Closed;
        self.camera.lock().unwrap().stop();

        Ok(())
    }

    fn generate_media_threads(&mut self, pair: &CandidatePair) -> Result<(), Error> {
        let remote_rtp: SocketAddr = format!("{}:{}", pair.remote.address, pair.remote.port)
            .parse()
            .map_err(|e| Error::ParsingSocketAddressError(e))?;
        let remote_rtcp: SocketAddr = format!("{}:{}", pair.remote.address, pair.remote.port + 1)
            .parse()
            .map_err(|e| Error::ParsingSocketAddressError(e))?;

        self.rtp_socket
            .connect(remote_rtp)
            .map_err(|e| Error::ConnectionSocketError(e.to_string()))?;
        self.rtcp_socket
            .connect(remote_rtcp)
            .map_err(|e| Error::ConnectionSocketError(e.to_string()))?;

        let rtp_sender_socket = self
            .rtp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;
        let rtcp_sender_socket = self
            .rtcp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;

        let rtp_receiver_socket = rtp_sender_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;
        let rtcp_receiver_socket = rtcp_sender_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;

        self.spawn_camera_thread()?;
        self.spawn_rtp_sender_thread(
            rtp_sender_socket,
            rtcp_sender_socket,
            Arc::clone(&self.connection_status),
        )?;
        self.spawn_rtp_receiver_thread(
            rtp_receiver_socket,
            rtcp_receiver_socket,
            Arc::clone(&self.connection_status),
        )?;
        self.handle_threads_errors();

        Ok(())
    }

    pub fn spawn_camera_thread(&mut self) -> Result<(), Error> {
        let tx_local_cam = self.tx_local.clone();
        let tx_encoded = self.tx_encoded.clone();
        let tx_thread = self.tx_thread.clone();
        let rx_camera = self.camera.lock().map_err(|_| Error::PoisonedLock)?.start();
        let status = self.connection_status.clone();

        let handler = thread::spawn({
            let mut encoder = Encoder::new().map_err(|e| Error::MapError(e.to_string()))?;
            move || {
                for frame in rx_camera {
                    if *status.read().unwrap() == ConnectionStatus::Closed {
                        break;
                    }
                    if let Err(e) = tx_local_cam.send(frame.clone()) {
                        let error = ThreadsError::Fatal(e.to_string());
                        tx_thread.send(error).unwrap();
                    }
                    let encoded = match encoder.encode_frame(&frame) {
                        Ok(enc) => enc,
                        Err(e) => {
                            let error = ThreadsError::Fatal(e.to_string());
                            tx_thread.send(error).unwrap();
                            return;
                        }
                    };

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
        Ok(())
    }

    fn spawn_rtp_sender_thread(
        &mut self,
        rtp_socket: UdpSocket,
        rtcp_socket: UdpSocket,
        connection_status: Arc<RwLock<ConnectionStatus>>,
    ) -> Result<(), Error> {
        let rx_encoded = self.rx_encoded.clone();
        let tx_thread = self.tx_thread.clone();
        let status = self.connection_status.clone();

        let handler = thread::spawn({
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
                    if *status.read().unwrap() == ConnectionStatus::Closed {
                        break;
                    }
                    let frame_lock = rx_encoded.lock().unwrap();
                    let encoded_frame = match frame_lock.recv() {
                        Ok(f) => f,
                        Err(e) => {
                            let error = ThreadsError::Fatal(e.to_string());
                            tx_thread.send(error).unwrap();
                            return;
                        }
                    };
                    for (i, c) in encoded_frame.chunks.iter().enumerate() {
                        if let Err(e) = rtp_sender.send(
                            c,
                            96,
                            Local::now().timestamp_millis() as u32,
                            encoded_frame.id,
                            i as u64,
                            encoded_frame.chunks.len() as u16,
                        ) {
                            let error = ThreadsError::Recoverable(e.to_string());
                            tx_thread.send(error).unwrap();
                        }
                    }
                }
            }
        });
        self.rtp_sender_handler = Some(handler);
        Ok(())
    }

    fn spawn_rtp_receiver_thread(
        &mut self,
        rtp_receiver_socket: UdpSocket,
        rtcp_receiver_socket: UdpSocket,
        connection_status: Arc<RwLock<ConnectionStatus>>,
    ) -> Result<(), Error> {
        let tx_remote_cam_receiver = self.tx_remote.clone();
        let tx_thread = self.tx_thread.clone();
        let status = self.connection_status.clone();

        let handler = thread::spawn({
            let mut receiver =
                RtpReceiver::new(rtp_receiver_socket, rtcp_receiver_socket, connection_status)
                    .map_err(|e| Error::RtpReceiverError(e.to_string()))?;
            let mut decoder = Decoder::new().map_err(|e| Error::MapError(e.to_string()))?;
            move || {
                let mut actual_frame = None;
                let mut chunks = Vec::new();

                loop {
                    if *status.read().unwrap() == ConnectionStatus::Closed {
                        break;
                    }
                    let rtp_packet = match receiver.receive() {
                        Ok(packet) => packet,
                        Err(e) => {
                            let error = ThreadsError::Fatal(e.to_string());
                            tx_thread.send(error).unwrap();
                            return;
                        }
                    };
                    match actual_frame {
                        Some(act_frame_id) => {
                            if act_frame_id == rtp_packet.frame_id {
                                chunks.push(rtp_packet.clone());
                            } else {
                                chunks = vec![rtp_packet.clone()];
                                actual_frame = Some(rtp_packet.frame_id);
                            }

                            if rtp_packet.marker == chunks.len() as u16 {
                                if let Some(frame_data) =
                                    generate_frame_from(&mut chunks, &mut decoder)
                                {
                                    if let Err(e) = tx_remote_cam_receiver.send(frame_data) {
                                        let error = ThreadsError::Fatal(e.to_string());
                                        tx_thread.send(error).unwrap();
                                    }
                                }
                            }
                        }
                        None => {
                            actual_frame = Some(rtp_packet.frame_id);
                            chunks.push(rtp_packet.clone());
                        }
                    }
                }
            }
        });
        self.rtp_receiver_handler = Some(handler);
        Ok(())
    }

    fn handle_threads_errors(&mut self) {
        let rx_thread = Arc::clone(&self.rx_thread);
        let connection_status = Arc::clone(&self.connection_status);

        thread::spawn(move || {
            loop {
                match rx_thread.lock().unwrap().recv() {
                    Ok(err) => match err {
                        ThreadsError::Recoverable(msg) => {
                            eprintln!("[WARN] Thread error (recoverable): {}", msg);
                        }
                        ThreadsError::Fatal(msg) => {
                            eprintln!("[FATAL] Thread error: {}", msg);
                            if let Ok(mut conn) = connection_status.write() {
                                *conn = ConnectionStatus::Closed;
                            }
                            break;
                        }
                    },
                    Err(_) => {
                        eprintln!("[ERROR] Monitor channel closed — all threads finished?");
                        break;
                    }
                }
            }

            eprintln!("Monitor thread exiting.");
        });
    }
}
fn generate_frame_from(chunks: &mut Vec<RtpPacket>, decoder: &mut Decoder) -> Option<Frame> {
    let fr_id = chunks.first()?.frame_id;

    // Sort the chunks to ensure correct NAL unit order (SPS, PPS, then video)
    chunks.sort_by_key(|c| c.chunk_id);
    let mut data = Vec::new();
    for c in chunks.iter() {
        data.extend_from_slice(&c.payload);
    }
    let (decoded_data, width, height) = decoder.decode_frame(&data).ok()?;

    Some(Frame {
        data: decoded_data,
        width,
        height,
        id: fr_id,
    })
}
