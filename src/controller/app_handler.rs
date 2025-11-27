use super::error::{ControllerError as Error, ThreadsError};
use crate::media::camera::{Camera, FrameSource};
use crate::session::CallSession;
use crate::config::Config;
use crate::frame_handler::{Decoder, EncodedFrame, Encoder, Frame};
use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtp::{RtpPacket, RtpReceiver, RtpSender};
use chrono::prelude::*;
use std::net::UdpSocket;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use crate::media::frame_handler::Frame;
use crate::transport::rtcp::RtcpReportHandler;

pub struct AppHandler {
    pub client: CallSession,
    pub config: Arc<Config>,

    //Channels
    pub tx_encoded: Sender<EncodedFrame>,
    pub rx_encoded: Arc<Mutex<Receiver<EncodedFrame>>>,
    pub tx_local: Sender<Frame>,
    pub tx_remote: Sender<Frame>,
    pub tx_thread: Sender<ThreadsError>,
    pub rx_thread: Arc<Mutex<Receiver<ThreadsError>>>,
    pub tx_event: Sender<String>,

    //Connection status
    pub connection_status: Arc<RwLock<ConnectionStatus>>,

    //Components
    pub camera: Arc<Mutex<Box<dyn FrameSource + Send>>>,
    pub rtcp_handler: Option<Arc<Mutex<RtcpReportHandler<UdpSocket>>>>,
    //
    //Sockets
    rtp_socket: UdpSocket,
    rtcp_socket: UdpSocket,
}

impl AppHandler {
    pub fn new(
        tx_local: Sender<Frame>,
        tx_remote: Sender<Frame>,
        tx_event: Sender<String>,
        config: Arc<Config>,
    ) -> Result<Self, Error> {
        let (tx_encoded, rx_encoded) = channel();
        let (tx_thread, rx_thread) = channel();

        let rtp_socket = UdpSocket::bind(format!("{}:0", config.network.bind_address))
            .map_err(|e| Error::MapError(e.to_string()))?;
        let rtp_port = rtp_socket
            .local_addr()
            .map_err(|e| Error::MapError(e.to_string()))?
            .port();

        let rtcp_addr = format!("{}:{}", config.network.bind_address, rtp_port + 1);
        let rtcp_socket =
            UdpSocket::bind(&rtcp_addr).map_err(|e| Error::MapError(e.to_string()))?;

        Ok(Self {
            client: CallSession::new(rtp_port, &config.media, &config.ice, &config.sdp)
                .map_err(|e| Error::MapError(e.to_string()))?,
            tx_encoded,
            rx_encoded: Arc::new(Mutex::new(rx_encoded)),
            tx_local,
            tx_remote,
            tx_thread,
            tx_event,
            rx_thread: Arc::new(Mutex::new(rx_thread)),
            connection_status: Arc::new(RwLock::new(ConnectionStatus::Closed)),
            camera: Arc::new(Mutex::new(Box::new(Camera::new(config.media.clone())))),
            rtcp_handler: None,
            rtp_socket,
            rtcp_socket,
            config,
        })
    }

    pub fn connect(&mut self) -> Result<(), Error> {
        let pair = match self.client.ice_agent.get_selected_pair().cloned() {
            Ok(pair) => pair,
            Err(e) => return Err(Error::MapError(e.to_string())),
        };

        let remote_rtp: std::net::SocketAddr =
            format!("{}:{}", pair.remote.address, pair.remote.port)
                .parse()
                .map_err(Error::ParsingSocketAddressError)?;
        let remote_rtcp: std::net::SocketAddr =
            format!("{}:{}", pair.remote.address, pair.remote.port + 1)
                .parse()
                .map_err(Error::ParsingSocketAddressError)?;
        
        self.rtp_socket
            .connect(remote_rtp)
            .map_err(|e| Error::ConnectionSocketError(e.to_string()))?;
        self.rtcp_socket
            .connect(remote_rtcp)
            .map_err(|e| Error::ConnectionSocketError(e.to_string()))?;

        let rtcp_handler = RtcpReportHandler::new(
            self.rtcp_socket
                .try_clone()
                .map_err(|e| Error::CloningSocketError(e.to_string()))?,
            Arc::clone(&self.connection_status),
            self.config.rtcp.clone(),
        );
        rtcp_handler
            .init_connection()
            .map_err(|e| Error::MapError(e.to_string()))?;
        self.rtcp_handler = Some(Arc::new(Mutex::new(rtcp_handler)));
        Ok(())
    }

    pub fn start_call(&mut self) -> Result<(), Error> {
        {
            let mut conn = self
                .connection_status
                .write()
                .map_err(|_| Error::PoisonedLock)?;
            *conn = ConnectionStatus::Waiting;
        }

        self.connect()?;

        match &self.rtcp_handler {
            Some(handler_lock) => {
                handler_lock
                    .lock()
                    .map_err(|_| Error::PoisonedLock)?
                    .start()
                    .map_err(|e| Error::MapError(e.to_string()))?;
            }
            None => return Err(Error::ConnectionNotStarted),
        }

        self.generate_media_threads()
    }

    pub fn shut_down(&mut self) -> Result<(), Error> {
        {
            self.camera
                .lock()
                .map_err(|_| Error::PoisonedLock)?
                .stop()?;
        }

        match &self.rtcp_handler {
            Some(handler_lock) => {
                handler_lock
                    .lock()
                    .map_err(|_| Error::PoisonedLock)?
                    .close_connection()
                    .map_err(|e| Error::MapError(e.to_string()))?;
            }
            None => return Err(Error::ConnectionNotStarted),
        }

        Ok(())
    }

    fn generate_media_threads(&mut self) -> Result<(), Error> {
        let rtp_sender_socket = self
            .rtp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;

        let rtp_receiver_socket = rtp_sender_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;

        self.spawn_camera_thread()?;

        self.spawn_rtp_sender_thread(rtp_sender_socket)?;
        self.spawn_rtp_receiver_thread(rtp_receiver_socket)?;
        self.handle_threads_errors();

        Ok(())
    }

    pub fn spawn_camera_thread(&mut self) -> Result<(), Error> {
        let tx_local_cam = self.tx_local.clone();
        let tx_encoded = self.tx_encoded.clone();
        let tx_thread = self.tx_thread.clone();
        let rx_camera = self
            .camera
            .lock()
            .map_err(|_| Error::PoisonedLock)?
            .start()?;
        let mut encoder =
            Encoder::new(&self.config.media).map_err(|e| Error::MapError(e.to_string()))?;

        thread::spawn(move || {
            for frame in rx_camera {
                if let Err(e) = tx_local_cam.send(frame.clone()) {
                    let error = ThreadsError::Fatal(e.to_string());
                    if tx_thread.send(error).is_err() {
                        eprintln!("[THREAD] Failed to send error to monitor, exiting thread");
                    }
                    break;
                }
                let encoded = match encoder.encode_frame(&frame) {
                    Ok(enc) => enc,
                    Err(e) => {
                        let error = ThreadsError::Fatal(e.to_string());
                        if tx_thread.send(error).is_err() {
                            eprintln!("[THREAD] Failed to send error to monitor, exiting thread");
                        }
                        break;
                    }
                };

                let encoded_frame = EncodedFrame {
                    id: frame.id,
                    chunks: encoded,
                    width: frame.width,
                    height: frame.height,
                };

                if tx_encoded.send(encoded_frame).is_err() {
                    break;
                }
            }
        });
        Ok(())
    }

    fn spawn_rtp_sender_thread(&self, rtp_socket: UdpSocket) -> Result<(), Error> {
        let rx_encoded = self.rx_encoded.clone();
        let tx_thread = self.tx_thread.clone();
        let status = self.connection_status.clone();
        let payload_type = self.config.media.rtp_payload_type;

        let rtcp_handler = match &self.rtcp_handler {
            Some(handler_lock) => Arc::clone(handler_lock),
            None => return Err(Error::ConnectionNotStarted),
        };

        let rtp_sender = RtpSender::new(
            rtp_socket,
            rtcp_handler,
            self.config.media.default_ssrc,
            Arc::clone(&self.connection_status),
            self.config.media.rtp_version,
        )
        .map_err(|e| Error::RtpSenderError(e.to_string()))?;
        let rtp_sender = Arc::new(Mutex::new(rtp_sender));

        thread::spawn({
            move || {
                loop {
                    if let Ok(is_closed) = connection_is_closed(&tx_thread, &status)
                        && is_closed
                    {
                        break;
                    }

                    let Ok(frame_lock) = rx_encoded.lock() else {
                        eprintln!("[THREAD] Failed to get receiver lock. Exiting thread");
                        break;
                    };
                    let encoded_frame = match frame_lock.recv() {
                        Ok(f) => f,
                        Err(e) => {
                            let error = ThreadsError::Fatal(e.to_string());
                            if tx_thread.send(error).is_err() {
                                eprintln!(
                                    "[THREAD] Failed to send error to monitor, exiting thread"
                                );
                            }
                            break;
                        }
                    };
                    for (i, c) in encoded_frame.chunks.iter().enumerate() {
                        let Ok(mut sender) = rtp_sender.lock() else {
                            let error = ThreadsError::Fatal(Error::PoisonedLock.to_string());
                            if tx_thread.send(error).is_err() {
                                eprintln!(
                                    "[THREAD] Failed to send error to monitor, exiting thread"
                                );
                            }
                            return;
                        };
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let marker = if let Ok(marker) = u16::try_from(encoded_frame.chunks.len()) {
                            marker
                        } else {
                            let error =
                                ThreadsError::Fatal("Too many chunks for RTP marker".to_string());
                            if tx_thread.send(error).is_err() {
                                eprintln!(
                                    "[THREAD] Failed to send error to monitor, exiting thread"
                                );
                            }
                            return;
                        };

                        if let Err(e) = sender.send(
                            c,
                            payload_type,
                            Local::now().timestamp_millis() as u32,
                            encoded_frame.id,
                            i as u64,
                            marker,
                        ) {
                            let error = ThreadsError::Fatal(e.to_string());
                            if tx_thread.send(error).is_err() {
                                eprintln!(
                                    "[THREAD] Failed to send error to monitor, exiting thread"
                                );
                            }
                            break;
                        }
                    }
                }
            }
        });
        Ok(())
    }

    fn spawn_rtp_receiver_thread(&self, rtp_receiver_socket: UdpSocket) -> Result<(), Error> {
        let tx_remote_cam_receiver = self.tx_remote.clone();
        let tx_thread = self.tx_thread.clone();
        let status = self.connection_status.clone();
        let rtp_timeout = self.config.rtcp.rtp_read_timeout_millis;

        let rtcp_handler = match &self.rtcp_handler {
            Some(handler_lock) => Arc::clone(handler_lock),
            None => return Err(Error::ConnectionNotStarted),
        };

        thread::spawn({
            let mut receiver = RtpReceiver::new(
                rtp_receiver_socket,
                rtcp_handler,
                Arc::clone(&self.connection_status),
                rtp_timeout,
                self.config.network.max_udp_packet_size,
            )
            .map_err(|e| Error::RtpReceiverError(e.to_string()))?;

            let mut decoder = Decoder::new().map_err(|e| Error::MapError(e.to_string()))?;

            move || {
                let mut actual_frame = None;
                let mut chunks = Vec::new();

                loop {
                    if let Ok(is_closed) = connection_is_closed(&tx_thread, &status)
                        && is_closed
                    {
                        break;
                    }
                    let rtp_packet = match receiver.receive() {
                        Ok(packet) => packet,
                        Err(e) => {
                            let error = ThreadsError::Fatal(e.to_string());
                            if tx_thread.send(error).is_err() {
                                eprintln!(
                                    "[THREAD] Failed to send error to monitor, exiting thread"
                                );
                            }
                            break;
                        }
                    };
                    // If this packet is for a new frame, reset the chunks
                    if actual_frame != Some(rtp_packet.frame_id) {
                        chunks = vec![rtp_packet.clone()];
                        actual_frame = Some(rtp_packet.frame_id);
                    } else {
                        // Otherwise, add it to the current frame's chunks
                        chunks.push(rtp_packet.clone());
                    }

                    // Check for frame completion *after* adding the chunk
                    let expected_marker = rtp_packet.marker;
                    let current_chunk_count = if let Ok(count) = u16::try_from(chunks.len()) {
                        count
                    } else {
                        let error =
                            ThreadsError::Fatal("Too many chunks for RTP marker".to_string());
                        if tx_thread.send(error).is_err() {
                            eprintln!("[THREAD] Failed to send error to monitor, exiting thread");
                        }
                        return;
                    };

                    // If the number of chunks we have matches the expected total (marker), process the frame
                    if current_chunk_count == expected_marker {
                        if let Some(frame_data) = generate_frame_from(&mut chunks, &mut decoder)
                            && let Err(e) = tx_remote_cam_receiver.send(frame_data) {
                                let error = ThreadsError::Fatal(e.to_string());
                                if tx_thread.send(error).is_err() {
                                    eprintln!(
                                        "[THREAD] Failed to send error to monitor, exiting thread"
                                    );
                                }
                                break;
                            }

                        // Reset for the next frame
                        actual_frame = None;
                        chunks.clear();
                    }
                }
            }
        });
        Ok(())
    }

    fn handle_threads_errors(&self) {
        let rx_thread = Arc::clone(&self.rx_thread);
        let connection_status = Arc::clone(&self.connection_status);
        let tx_event = self.tx_event.clone();

        thread::spawn(move || {
            loop {
                let value = if let Ok(rx) = rx_thread.lock() {
                    rx.recv()
                } else {
                    eprintln!("[ERROR] Monitor thread receiver lock poisoned, exiting");
                    break;
                };
                if let Ok(err) = value {
                    match err {
                        ThreadsError::Recoverable(msg) => {
                            eprintln!("[WARN] Thread error (recoverable): {msg}");
                        }
                        ThreadsError::Fatal(msg) => {
                            eprintln!("[FATAL] Thread error: {msg}");
                            if let Ok(mut conn) = connection_status.write() {
                                *conn = ConnectionStatus::Closed;
                            }
                            if tx_event.send(msg).is_err() {
                                eprintln!(
                                    "[THREAD] Failed to send error to interface, exiting thread"
                                );
                            }
                            break;
                        }
                    }
                } else {
                    eprintln!("[ERROR] Monitor channel closed — all threads finished?");
                    break;
                }
            }

            eprintln!("Monitor thread exiting.");
        });
    }

    pub fn reset(
        &mut self,
        tx_local: Sender<Frame>,
        tx_remote: Sender<Frame>,
        tx_event: Sender<String>,
    ) -> Result<(), Error> {
        let (tx_encoded, rx_encoded) = channel();
        let (tx_thread, rx_thread) = channel();

        self.tx_local = tx_local;
        self.tx_remote = tx_remote;
        self.tx_event = tx_event;
        self.tx_encoded = tx_encoded;
        self.rx_encoded = Arc::new(Mutex::new(rx_encoded));
        self.tx_thread = tx_thread;
        self.rx_thread = Arc::new(Mutex::new(rx_thread));

        self.client.ice_agent.clean_remote_candidates();

        Ok(())
    }

    pub fn stop_local_camera(&mut self) -> Result<(), Error> {
        self.camera
            .lock()
            .map_err(|_| Error::PoisonedLock)?
            .stop()?;
        Ok(())
    }
}
fn generate_frame_from(chunks: &mut [RtpPacket], decoder: &mut Decoder) -> Option<Frame> {
    let fr_id = chunks.first()?.frame_id;

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

fn connection_is_closed(
    tx_thread: &Sender<ThreadsError>,
    status: &Arc<RwLock<ConnectionStatus>>,
) -> Result<bool, Error> {
    if *status.read().map_err(|_| Error::PoisonedLock)? == ConnectionStatus::Closed {
        if tx_thread
            .send(ThreadsError::Fatal(Error::ConnectionClosed.to_string()))
            .is_err()
        {
            eprintln!("[THREAD] Failed to send error to monitor, exiting thread");
        }
        return Ok(true);
    }
    Ok(false)
}
