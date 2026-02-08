use crate::config::Config;
use crate::controller::AppEvent;
use crate::dtls::dtls_socket::DtlsSocket;
use crate::dtls::key_manager::LocalCert;
use crate::logger::Logger;
use crate::session::sdp::{DtlsSetupRole, Fingerprint};
use crate::srtp::context::SrtpContext;
use crate::transport::MediaTransportError as Error;
use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtcp::metrics::{ReceiverStats, SenderStats};
use crate::transport::rtp::{RtpPacket, RtpReceiver, RtpSender};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

/// Transport handles returned from `MediaTransport::start()` containing all necessary channels and state.
///
/// This structure provides access to the RTP send/receive channels, connection status,
/// and receiver statistics after starting the media transport.
pub struct TransportHandles {
    /// Channel for sending RTP packets to the remote peer
    pub rtp_tx: Sender<RtpPacket>,
    /// Channel for receiving RTP packets from the remote peer
    pub rtp_rx: Receiver<RtpPacket>,
    /// Shared flag indicating if the transport is connected
    pub is_connected: Arc<AtomicBool>,
    /// Receiver statistics for monitoring media quality
    pub receiver_stats: Arc<Mutex<ReceiverStats>>,
}

/// Media transport layer managing RTP/RTCP communication over DTLS/SRTP.
///
/// This struct orchestrates the complete media transport pipeline including:
/// - DTLS handshake for secure key exchange
/// - SRTP encryption/decryption of RTP packets
/// - RTP sender and receiver threads
/// - RTCP reporting for quality monitoring
///
/// The transport binds to local UDP sockets for RTP and RTCP (port+1),
/// establishes a DTLS connection with the remote peer, derives SRTP keys,
/// and spawns background threads for sending and receiving encrypted media.
pub struct MediaTransport {
    /// Application configuration (network, RTCP, RTP settings).
    config: Arc<Config>,

    /// Local address where the RTP socket is bound.
    pub rtp_address: SocketAddr,

    /// UDP socket for RTP media packets.
    pub rtp_socket: UdpSocket,
    /// UDP socket for RTCP control packets (typically RTP port + 1).
    rtcp_socket: UdpSocket,

    /// RTCP report handler managing periodic connectivity reports.
    rtcp_handler: Option<Arc<Mutex<RtcpReportHandler<UdpSocket>>>>,

    /// Shared connection status flag coordinating thread lifecycle.
    connected: Arc<AtomicBool>,
    /// Logger instance for transport-level logging.
    logger: Logger,
}

impl MediaTransport {
    /// Create a new `MediaTransport` instance by binding local RTP and RTCP sockets.
    ///
    /// Binds a UDP socket to an ephemeral port for RTP and another socket to port+1
    /// for RTCP. The bind address is taken from the application configuration.
    ///
    /// # Parameters
    /// - `config`: application configuration containing network settings.
    /// - `logger`: logger instance for transport-level messages.
    ///
    /// # Returns
    /// A configured `MediaTransport` ready to `start()`, or an error if binding fails.
    ///
    /// # Errors
    /// Returns `Error::BindingError` if socket binding fails.
    pub fn new(config: &Arc<Config>, logger: Logger) -> Result<Self, Error> {
        let rtp_socket = UdpSocket::bind(format!("{}:0", config.network.bind_address))
            .map_err(|e| Error::BindingError(e.to_string()))?;

        let rtp_address = rtp_socket
            .local_addr()
            .map_err(|e| Error::BindingError(e.to_string()))?;

        let rtcp_address = SocketAddr::new(rtp_address.ip(), rtp_address.port() + 1);
        let rtcp_socket =
            UdpSocket::bind(rtcp_address).map_err(|e| Error::BindingError(e.to_string()))?;

        logger.info(&format!(
            "MediaTransport bound to RTP: {rtp_address}, RTCP: {rtcp_address}"
        ));

        Ok(Self {
            config: Arc::clone(config),
            rtp_address,
            rtp_socket,
            rtcp_socket,
            rtcp_handler: None,
            connected: Arc::new(AtomicBool::new(false)),
            logger,
        })
    }

    /// Start the media transport by establishing DTLS, deriving SRTP keys, and spawning RTP/RTCP threads.
    ///
    /// This method performs the following steps:
    /// 1. Establishes a DTLS connection with the remote peer and verifies the fingerprint
    /// 2. Exports keying material and initializes SRTP context
    /// 3. Starts the RTCP report handler for connectivity monitoring
    /// 4. Spawns RTP sender and receiver threads with SRTP protection/unprotection layers
    ///
    /// # Parameters
    /// - `remote_rtp_address`: remote peer's RTP socket address.
    /// - `remote_rtcp_address`: remote peer's RTCP socket address.
    /// - `event_tx`: channel for application events (stats updates, call end).
    /// - `local_setup_role`: DTLS setup role (active/passive/actpass/holdconn).
    /// - `expected_fingerprint`: peer's certificate fingerprint for verification.
    /// - `local_cert`: local certificate for DTLS handshake.
    ///
    /// # Returns
    /// A `TransportHandles` struct containing:
    /// - `rtp_tx`: Sender channel for outgoing RTP packets
    /// - `rtp_rx`: Receiver channel for incoming RTP packets
    /// - `is_connected`: Shared connection status flag
    /// - `receiver_stats`: Shared receiver statistics
    ///
    /// # Errors
    /// Returns an error if DTLS handshake, key export, SRTP setup, or thread spawning fails.
    pub fn start(
        &mut self,
        remote_rtp_address: SocketAddr,
        remote_rtcp_address: SocketAddr,
        event_tx: Sender<AppEvent>,
        local_setup_role: DtlsSetupRole,
        expected_fingerprint: Fingerprint,
        local_cert: &LocalCert,
    ) -> Result<TransportHandles, Error> {
        self.logger.info(&format!(
            "Starting MediaTransport. Remote RTP: {remote_rtp_address}, Remote RTCP: {remote_rtcp_address}"
        ));
        let rtp_dtls = DtlsSocket::new(
            self.rtp_socket
                .try_clone()
                .map_err(|e| Error::CloningSocketError(e.to_string()))?,
            remote_rtp_address,
            local_setup_role,
            expected_fingerprint,
            local_cert,
        ).map_err(|e| Error::MapError(e.to_string()))?;
        self.connect_sockets(remote_rtp_address, remote_rtcp_address)?;
        let srtp_ctx = self.generate_srtp_ctx(rtp_dtls)?;
        let local_sender_stats = Arc::new(Mutex::new(SenderStats::default()));
        let local_receiver_stats = Arc::new(Mutex::new(ReceiverStats::default()));
        self.initialize_report_handler(
            local_sender_stats.clone(),
            local_receiver_stats.clone(),
            event_tx.clone(),
        )?;
        let (local_to_remote_rtp_tx, remote_to_local_rtp_rx) =
            self.spawn_rtp_threads(event_tx, srtp_ctx, local_setup_role, local_sender_stats)?;
        Ok(TransportHandles {
            rtp_tx: local_to_remote_rtp_tx,
            rtp_rx: remote_to_local_rtp_rx,
            is_connected: self.connected.clone(),
            receiver_stats: local_receiver_stats,
        })
    }

    /// Stop the media transport by closing the connection and sending RTCP goodbye.
    ///
    /// This method sets the connection flag to false (signaling all threads to terminate),
    /// clears socket timeouts, and sends an RTCP goodbye packet to notify the remote peer.
    ///
    /// # Returns
    /// `Ok(())` on success, or an error if socket configuration or goodbye transmission fails.
    ///
    /// # Errors
    /// Returns `Error::SocketConfigFailed` if clearing timeouts fails, or other errors
    /// if RTCP goodbye transmission fails.
    pub fn stop(&mut self) -> Result<(), Error> {
        self.logger.info("Stopping MediaTransport...");
        self.connected.store(false, Ordering::SeqCst);
        self.rtp_socket
            .set_read_timeout(None)
            .map_err(|_| Error::SocketConfigFailed)?;
        self.rtcp_socket
            .set_read_timeout(None)
            .map_err(|_| Error::SocketConfigFailed)?;

        if let Some(rtcp_handler) = &self.rtcp_handler
            && let Ok(rtcp_handler) = rtcp_handler.lock()
        {
            return rtcp_handler
                .report_goodbye()
                .map_err(|e| Error::MapError(e.to_string()));
        }

        Ok(())
    }

    /// Spawn RTP sender and receiver threads with SRTP protection/unprotection layers.
    ///
    /// This internal method creates a four-layer pipeline for each direction:
    /// - Outgoing: Application → Protection (encrypt) → Sender (network)
    /// - Incoming: Receiver (network) → Unprotection (decrypt) → Application
    ///
    /// # Parameters
    /// - `event_tx`: channel for application events.
    /// - `srtp_ctx`: SRTP context for encryption/decryption.
    /// - `role`: DTLS role determining key usage.
    /// - `sender_metrics`: shared sender statistics for RTCP reporting.
    ///
    /// # Returns
    /// A tuple of (sender channel for unprotected packets, receiver channel for unprotected packets).
    ///
    /// # Errors
    /// Returns an error if RTCP handler is not initialized or thread spawning fails.
    fn spawn_rtp_threads(
        &self,
        event_tx: Sender<AppEvent>,
        srtp_ctx: SrtpContext,
        role: DtlsSetupRole,
        sender_metrics: Arc<Mutex<SenderStats>>,
    ) -> Result<(Sender<RtpPacket>, Receiver<RtpPacket>), Error> {
        let rtcp_handler = match &self.rtcp_handler {
            Some(handler_lock) => Arc::clone(handler_lock),
            None => return Err(Error::ConnectionNotStarted),
        };

        let local_protected_data_tx = self.start_srtp_sender(&rtcp_handler, sender_metrics)?;

        let remote_protected_data_rx = self.start_srtp_receiver(&rtcp_handler, event_tx)?;

        let srtp_ctx = Arc::new(Mutex::new(srtp_ctx));
        let is_client = matches!(role, DtlsSetupRole::Active);

        let remote_unprotected_data_rx =
            self.start_unprotection_layer(remote_protected_data_rx, srtp_ctx.clone(), is_client);
        let local_unprotected_data_tx =
            self.start_protection_layer(local_protected_data_tx, srtp_ctx.clone(), is_client);

        Ok((local_unprotected_data_tx, remote_unprotected_data_rx))
    }

    /// Start the SRTP unprotection (decryption) layer for incoming packets.
    ///
    /// Spawns a thread that receives protected (encrypted) RTP packets from the network,
    /// decrypts them using the SRTP context, deserializes them into `RtpPacket` structs,
    /// and forwards them to the application layer.
    ///
    /// # Parameters
    /// - `protected_rx`: receiver channel for incoming encrypted RTP data.
    /// - `srtp_ctx`: shared SRTP context for decryption.
    /// - `is_client`: whether this endpoint is the DTLS client (affects key selection).
    ///
    /// # Returns
    /// A receiver channel for decrypted `RtpPacket` instances.
    fn start_unprotection_layer(
        &self,
        protected_rx: Receiver<Vec<u8>>,
        srtp_ctx: Arc<Mutex<SrtpContext>>,
        is_client: bool,
    ) -> Receiver<RtpPacket> {
        let (unprotected_tx, unprotected_rx) = mpsc::channel();
        let srtp_ctx = srtp_ctx.clone();
        let logger = self.logger.clone();

        thread::spawn(move || {
            for protected_data in protected_rx {
                if protected_data.is_empty() || !is_rtp_packet(protected_data[0]){
                    continue;
                }

                match unprotect_packet(&protected_data, &srtp_ctx, is_client) {
                    Ok(packet) => {
                        if let Err(_) = send_unprotected_packet(packet, &unprotected_tx, &logger) {
                            break
                        }
                    },
                    Err(e) => {
                        logger.error(&format!("SRTP unprotect failed: {e}"));
                        break;
                    }
                }
            }
            logger.info("Unprotection layer thread terminated");
        });

        unprotected_rx
    }
    /// Start the SRTP protection (encryption) layer for outgoing packets.
    ///
    /// Spawns a thread that receives unprotected `RtpPacket` instances from the application,
    /// encrypts them using the SRTP context, and forwards the encrypted data to the network
    /// sender thread.
    ///
    /// # Parameters
    /// - `protected_tx`: sender channel for encrypted RTP data to be sent over the network.
    /// - `srtp_ctx`: shared SRTP context for encryption.
    /// - `is_client`: whether this endpoint is the DTLS client (affects key selection).
    ///
    /// # Returns
    /// A sender channel for unprotected `RtpPacket` instances from the application.
    fn start_protection_layer(
        &self,
        protected_tx: Sender<Vec<u8>>,
        srtp_ctx: Arc<Mutex<SrtpContext>>,
        is_client: bool,
    ) -> Sender<RtpPacket> {
        let (unprotected_tx, unprotected_rx) = mpsc::channel();
        let srtp_ctx = srtp_ctx.clone();
        let logger = self.logger.clone();

        thread::spawn(move || {
            for unprotected_pkt in unprotected_rx {
                let protected_data = {
                    let mut srtp_ctx = match srtp_ctx.lock() {
                        Ok(c) => c,
                        Err(_) => break,
                    };

                    match srtp_ctx.protect(&unprotected_pkt, is_client) {
                        Ok(data) => data,
                        Err(e) => {
                            logger.error(&Error::ProtectionError(e.to_string()).to_string());
                            break;
                        }
                    }
                };

                if let Err(e) = protected_tx.send(protected_data) {
                    logger.error(&Error::ChannelSendError(e.to_string()).to_string());
                    break;
                }
            }
        });

        unprotected_tx
    }
    /// Start the SRTP sender for transmitting encrypted RTP packets to the network.
    ///
    /// Creates an `RtpSender` instance with a cloned socket and starts its background thread.
    /// The sender receives encrypted RTP data and transmits it to the remote peer while
    /// updating sender statistics for RTCP reporting.
    ///
    /// # Parameters
    /// - `rtcp_handler`: RTCP report handler for session management.
    /// - `sender_metrics`: shared sender statistics.
    ///
    /// # Returns
    /// A sender channel for encrypted RTP data.
    ///
    /// # Errors
    /// Returns an error if socket cloning or sender initialization fails.
    fn start_srtp_sender(
        &self,
        rtcp_handler: &Arc<Mutex<RtcpReportHandler<UdpSocket>>>,
        sender_metrics: Arc<Mutex<SenderStats>>,
    ) -> Result<Sender<Vec<u8>>, Error> {
        let srtp_sender_socket = self
            .rtp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;

        let srtp_sender = RtpSender::new(
            srtp_sender_socket,
            rtcp_handler,
            &self.connected,
            sender_metrics,
            self.logger.context("RtpSender"),
        )
            .map_err(|e| Error::MapError(e.to_string()))?;

        srtp_sender
            .start()
            .map_err(|e| Error::MapError(e.to_string()))
    }

    /// Start the SRTP receiver for receiving encrypted RTP packets from the network.
    ///
    /// Creates an `RtpReceiver` instance with a cloned socket and starts its background thread.
    /// The receiver listens for incoming encrypted RTP data and forwards it to the
    /// unprotection layer for decryption.
    ///
    /// # Parameters
    /// - `rtcp_handler`: RTCP report handler for session management.
    /// - `event_tx`: channel for application events.
    ///
    /// # Returns
    /// A receiver channel for encrypted RTP data from the network.
    ///
    /// # Errors
    /// Returns an error if socket cloning or receiver initialization fails.
    fn start_srtp_receiver(
        &self,
        rtcp_handler: &Arc<Mutex<RtcpReportHandler<UdpSocket>>>,
        event_tx: Sender<AppEvent>,
    ) -> Result<Receiver<Vec<u8>>, Error> {
        let srtp_receiver_socket = self
            .rtp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;

        let mut srtp_receiver = RtpReceiver::new(
            &self.config,
            srtp_receiver_socket,
            rtcp_handler,
            &self.connected,
            self.logger.context("RtpReceiver"),
        )
            .map_err(|e| Error::MapError(e.to_string()))?;

        srtp_receiver
            .start(event_tx)
            .map_err(|e| Error::MapError(e.to_string()))
    }

    fn connect_sockets(
        &mut self,
        remote_rtp_addr: SocketAddr,
        remote_rtcp_addr: SocketAddr,
    ) -> Result<(), Error> {
        self.rtp_socket
            .connect(remote_rtp_addr)
            .map_err(|e| Error::SocketConnectionError(e.to_string()))?;
        self.rtcp_socket
            .connect(remote_rtcp_addr)
            .map_err(|e| Error::SocketConnectionError(e.to_string()))?;

        self.logger.info("Sockets connected to remote addresses.");
        Ok(())
    }

    fn generate_srtp_ctx(
        &mut self,
        rtp_dtls: DtlsSocket
    ) -> Result<SrtpContext, Error> {
        let key_material = rtp_dtls
            .export_keying_material("EXTRACTOR-dtls_srtp", 60)
            .map_err(|e| Error::MapError(format!("Key export failed: {e}")))?;

        Ok(SrtpContext::new(&key_material)
            .map_err(|e| Error::MapError(format!("SRTP context creation failed: {e}")))?)
    }

    fn initialize_report_handler(
        &mut self,
        local_sender_stats: Arc<Mutex<SenderStats>>,
        local_receiver_stats: Arc<Mutex<ReceiverStats>>,
        event_tx: Sender<AppEvent>,
    ) -> Result<(), Error> {
        let rtcp_handler = RtcpReportHandler::new(
            self.rtcp_socket
                .try_clone()
                .map_err(|e| Error::CloningSocketError(e.to_string()))?,
            Arc::clone(&self.connected),
            self.config.rtcp.clone(),
            local_sender_stats.clone(),
            local_receiver_stats.clone(),
        );
        rtcp_handler
            .start(event_tx.clone())
            .map_err(|e| Error::MapError(e.to_string()))?;
        self.rtcp_handler = Some(Arc::new(Mutex::new(rtcp_handler)));
        Ok(())
    }
}

//Aux functions -----------------------------------------------------------------------------------

fn is_rtp_packet(first_byte: u8) -> bool {
    (128..=191).contains(&first_byte)
}

fn unprotect_packet(
    data: &[u8],
    ctx: &Arc<Mutex<SrtpContext>>,
    is_client: bool,
) -> Result<RtpPacket, String> {
    let mut srtp_ctx = ctx.lock().map_err(|e| format!("Lock failed: {e}"))?;
    srtp_ctx.unprotect(data, is_client).map_err(|e| e.to_string())
}

fn send_unprotected_packet(
    packet: RtpPacket,
    tx: &Sender<RtpPacket>,
    logger: &Logger,
) -> Result<(), Error> {
    tx.send(packet).map_err({
        logger.error(&"Failed to send unprotected RTP packet".to_string());
        |e| Error::MapError(e.to_string())
    })
}
