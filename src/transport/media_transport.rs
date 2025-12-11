use crate::config::Config;
use crate::controller::AppEvent;
use crate::dtls::dtls_socket::DtlsSocket;
use crate::dtls::key_manager::{LocalCert, PKCS12_PASSWORD};
use crate::logger::Logger;
use crate::session::sdp::{DtlsSetupRole, Fingerprint};
use crate::srtp::context::SrtpContext;
use crate::transport::MediaTransportError as Error;
use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtcp::metrics::{ReceiverStats, SenderStats};
use crate::transport::rtp::{RtpPacket, RtpReceiver, RtpSender};
use openssl::pkcs12::Pkcs12;
use openssl::ssl::{SslAcceptor, SslMethod, SslVerifyMode};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use udp_dtls::{DtlsAcceptor, DtlsConnector, DtlsStream, SignatureAlgorithm, UdpChannel};

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

        let rtp_dtls = self.create_dtls_socket(
            self.rtp_socket
                .try_clone()
                .map_err(|e| Error::CloningSocketError(e.to_string()))?,
            remote_rtp_address,
            local_setup_role,
            expected_fingerprint,
            local_cert,
        )?;

        self.rtp_socket
            .connect(remote_rtp_address)
            .map_err(|e| Error::SocketConnectionError(e.to_string()))?;
        self.rtcp_socket
            .connect(remote_rtcp_address)
            .map_err(|e| Error::SocketConnectionError(e.to_string()))?;

        self.logger.info("Sockets connected to remote addresses.");

        let key_material = rtp_dtls
            .export_keying_material("EXTRACTOR-dtls_srtp", 60)
            .map_err(|e| Error::MapError(format!("Key export failed: {e}")))?;

        let srtp_context = SrtpContext::new(&key_material)
            .map_err(|e| Error::MapError(format!("SRTP context creation failed: {e}")))?;

        let local_sender_stats = Arc::new(Mutex::new(SenderStats::default()));
        let local_receiver_stats = Arc::new(Mutex::new(ReceiverStats::default()));

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

        let (local_to_remote_rtp_tx, remote_to_local_rtp_rx) =
            self.spawn_rtp_threads(event_tx, srtp_context, local_setup_role, local_sender_stats)?;

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
                if protected_data.is_empty() {
                    continue;
                }
                let first_byte = protected_data[0];

                if (20..=63).contains(&first_byte) {
                    continue;
                } else if (128..=191).contains(&first_byte) {
                    match srtp_ctx.lock() {
                        Ok(mut srtp_ctx) => match srtp_ctx.unprotect(&protected_data, is_client) {
                            Ok(unprotected_packet) => {
                                if let Err(e) = unprotected_tx.send(unprotected_packet) {
                                    logger.error(&format!(
                                        "Failed to send unprotected RTP packet: {e}"
                                    ));
                                    break;
                                }
                            }
                            Err(e) => {
                                logger.error(&format!("SRTP unprotect failed: {e}"));
                                break;
                            }
                        },
                        Err(e) => {
                            logger.error(&format!("SRTP context lock failed: {e}"));
                            break;
                        }
                    }
                } else {
                    continue;
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

    /// Create a DTLS socket by performing handshake with the remote peer.
    ///
    /// Normalizes the DTLS setup role (actpass → active, holdconn → passive),
    /// performs the DTLS handshake as client or server, and verifies the peer's
    /// certificate fingerprint against the expected value from SDP.
    ///
    /// # Parameters
    /// - `socket`: UDP socket for DTLS communication.
    /// - `remote_addr`: remote peer's socket address.
    /// - `local_setup_role`: DTLS setup role from SDP negotiation.
    /// - `expected_fingerprint`: peer's certificate fingerprint for verification.
    /// - `local_cert`: local certificate for DTLS handshake.
    ///
    /// # Returns
    /// A `DtlsSocket` wrapper over the established DTLS connection.
    ///
    /// # Errors
    /// Returns an error if handshake or fingerprint verification fails.
    #[allow(deprecated)]
    fn create_dtls_socket(
        &self,
        socket: UdpSocket,
        remote_addr: SocketAddr,
        local_setup_role: DtlsSetupRole,
        expected_fingerprint: Fingerprint,
        local_cert: &LocalCert,
    ) -> Result<DtlsSocket, Error> {
        let mut role = local_setup_role;
        if matches!(role, DtlsSetupRole::ActPass) {
            role = DtlsSetupRole::Active;
        } else if matches!(role, DtlsSetupRole::HoldConn) {
            role = DtlsSetupRole::Passive;
        }

        let channel = UdpChannel {
            socket,
            remote_addr,
        };

        let stream = match role {
            DtlsSetupRole::Active => {
                let identity = local_cert
                    .duplicate_identity()
                    .map_err(|e| Error::MapError(e.to_string()))?;

                let connector = DtlsConnector::builder()
                    .identity(identity)
                    .danger_accept_invalid_certs(true)
                    .danger_accept_invalid_hostnames(true)
                    .build()
                    .map_err(|e| Error::SocketConnectionError(e.to_string()))?;

                connector
                    .connect("roomrtc.local", channel)
                    .map_err(|e| Error::SocketConnectionError(format!("{e:?}")))?
            }
            DtlsSetupRole::Passive => {
                let pkcs12 = Pkcs12::from_der(&local_cert.pkcs12_der)
                    .map_err(|e| Error::MapError(e.to_string()))?
                    .parse(PKCS12_PASSWORD)
                    .map_err(|e| Error::MapError(e.to_string()))?;

                let mut acceptor_builder = SslAcceptor::mozilla_intermediate(SslMethod::dtls())
                    .map_err(|e| Error::MapError(e.to_string()))?;

                acceptor_builder
                    .set_private_key(&pkcs12.pkey)
                    .map_err(|e| Error::MapError(e.to_string()))?;
                acceptor_builder
                    .set_certificate(&pkcs12.cert)
                    .map_err(|e| Error::MapError(e.to_string()))?;
                acceptor_builder
                    .check_private_key()
                    .map_err(|e| Error::MapError(e.to_string()))?;

                acceptor_builder.set_verify_callback(
                    SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT,
                    |_, _| true,
                );

                let ssl_acceptor = acceptor_builder.build();
                let acceptor = DtlsAcceptor(ssl_acceptor);

                acceptor
                    .accept(channel)
                    .map_err(|e| Error::SocketConnectionError(format!("{e:?}")))?
            }
            _ => unreachable!("DTLS role should be normalized before handshake"),
        };

        self.verify_peer_fingerprint(&stream, &expected_fingerprint)?;

        Ok(DtlsSocket::new(stream, remote_addr))
    }

    /// Verify the peer's certificate fingerprint against the expected value from SDP.
    ///
    /// Extracts the peer's certificate from the DTLS stream, computes its SHA-256
    /// fingerprint, and compares it with the fingerprint advertised in the SDP offer/answer.
    /// This prevents man-in-the-middle attacks by ensuring the certificate matches.
    ///
    /// # Parameters
    /// - `stream`: established DTLS stream.
    /// - `expected`: fingerprint from SDP (algorithm and bytes).
    ///
    /// # Returns
    /// `Ok(())` if the fingerprint matches, otherwise an error.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The fingerprint algorithm is not SHA-256
    /// - The peer certificate is missing
    /// - The computed fingerprint doesn't match the expected value
    fn verify_peer_fingerprint(
        &self,
        stream: &DtlsStream<UdpChannel>,
        expected: &Fingerprint,
    ) -> Result<(), Error> {
        if !expected.algorithm().eq_ignore_ascii_case("sha-256") {
            return Err(Error::MapError(format!(
                "Unsupported fingerprint algorithm: {}",
                expected.algorithm()
            )));
        }

        let certificate = stream
            .peer_certificate()
            .map_err(|e| Error::MapError(e.to_string()))?
            .ok_or_else(|| Error::MapError("Peer certificate missing".to_string()))?;
        let fingerprint = certificate
            .fingerprint(SignatureAlgorithm::Sha256)
            .map_err(|e| Error::MapError(e.to_string()))?;
        if fingerprint.bytes != expected.bytes() {
            return Err(Error::MapError(
                "Peer certificate fingerprint mismatch".to_string(),
            ));
        }
        Ok(())
    }
}
