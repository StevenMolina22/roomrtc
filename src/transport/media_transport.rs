use crate::config::Config;
use crate::transport::MediaTransportError as Error;
use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtp::{RtpPacket, RtpReceiver, RtpSender};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use crate::controller::AppEvent;
use crate::srtp::context::SrtpContext;
use crate::dtls::dtls_socket::DtlsSocket;
use udp_dtls::{DtlsAcceptor, DtlsConnector, DtlsStream, SignatureAlgorithm, UdpChannel};
use crate::session::sdp::{DtlsSetupRole, Fingerprint};
use openssl::pkcs12::Pkcs12;
use crate::dtls::key_manager::{LocalCert, PKCS12_PASSWORD};
use openssl::ssl::{SslAcceptor, SslMethod, SslVerifyMode};

pub struct MediaTransport {
    config: Arc<Config>,

    pub rtp_address: SocketAddr,

    pub rtp_socket: UdpSocket,
    rtcp_socket: UdpSocket,

    rtcp_handler: Option<Arc<Mutex<RtcpReportHandler<UdpSocket>>>>,

    connected: Arc<AtomicBool>,
}

impl MediaTransport {
    pub fn new(config: &Arc<Config>) -> Result<Self, Error> {
        let rtp_socket = UdpSocket::bind(format!("{}:0", config.network.bind_address))
            .map_err(|e| Error::BindingError(e.to_string()))?;

        let rtp_address = rtp_socket
            .local_addr()
            .map_err(|e| Error::BindingError(e.to_string()))?;

        let rtcp_address = SocketAddr::new(rtp_address.ip(), rtp_address.port() + 1);
        let rtcp_socket =
            UdpSocket::bind(rtcp_address).map_err(|e| Error::BindingError(e.to_string()))?;

        Ok(Self {
            config: Arc::clone(config),
            rtp_address,
            rtp_socket,
            rtcp_socket,
            rtcp_handler: None,
            connected: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn start(
        &mut self,
        remote_rtp_address: SocketAddr,
        remote_rtcp_address: SocketAddr,
        event_tx: Sender<AppEvent>,
        local_setup_role: DtlsSetupRole,
        expected_fingerprint: Fingerprint,
        local_cert: &LocalCert
    ) -> Result<(Sender<RtpPacket>, Receiver<RtpPacket>, Arc<AtomicBool>), Error> {
        let rtp_dtls = self.create_dtls_socket(
            self.rtp_socket
                .try_clone()
                .map_err(|e| Error::CloningSocketError(e.to_string()))?,
            remote_rtp_address,
            local_setup_role.clone(),
            expected_fingerprint.clone(),
            local_cert
        )?;

        self.rtp_socket
            .connect(remote_rtp_address)
            .map_err(|e| Error::SocketConnectionError(e.to_string()))?;
        self.rtcp_socket
            .connect(remote_rtcp_address)
            .map_err(|e| Error::SocketConnectionError(e.to_string()))?;

        // Export Keys & Setup Context
        let key_material = rtp_dtls
            .export_keying_material("EXTRACTOR-dtls_srtp", 60)
            .map_err(|e| Error::MapError(format!("Key export failed: {}", e)))?;
        println!("DTLS-SRTP Key Material: {:?}", key_material);

        let srtp_context = SrtpContext::new(&key_material)
            .map_err(|e| Error::MapError(format!("SRTP context creation failed: {}", e)))?;

        let local_ssrc = self.config.media.default_ssrc;

        let rtcp_handler = RtcpReportHandler::new(
            self.rtcp_socket
                .try_clone()
                .map_err(|e| Error::CloningSocketError(e.to_string()))?,
            Arc::clone(&self.connected),
            self.config.rtcp.clone(),
            local_ssrc,
        );

        rtcp_handler
            .start(event_tx.clone())
            .map_err(|e| Error::MapError(e.to_string()))?;
        self.rtcp_handler = Some(Arc::new(Mutex::new(rtcp_handler)));

        let (local_to_remote_rtp_tx, remote_to_local_rtp_rx) = self.spawn_rtp_threads(event_tx, local_setup_role.clone(), srtp_context)?;

        Ok((local_to_remote_rtp_tx, remote_to_local_rtp_rx, self.connected.clone()))
    }



    pub fn stop(&mut self) -> Result<(), Error> {
        self.connected.store(false, Ordering::SeqCst);
        self.rtp_socket.set_read_timeout(None).map_err(|e| Error::SocketConfigFailed)?;
        self.rtcp_socket.set_read_timeout(None).map_err(|e| Error::SocketConfigFailed)?;

        if let Some(rtcp_handler) = &self.rtcp_handler
            && let Ok(rtcp_handler) = rtcp_handler.lock() {
                return rtcp_handler
                    .report_goodbye()
                    .map_err(|e| Error::MapError(e.to_string()));
        }

        Ok(())
    }

    fn spawn_rtp_threads(
        &self,
        event_tx: Sender<AppEvent>,
        role: DtlsSetupRole,
        srtp_context: SrtpContext,
    ) -> Result<(Sender<RtpPacket>, Receiver<RtpPacket>), Error> {
        let rtcp_handler = match &self.rtcp_handler {
            Some(handler_lock) => Arc::clone(handler_lock),
            None => return Err(Error::ConnectionNotStarted),
        };

        let is_client = matches!(role, DtlsSetupRole::Active);
        let srtp_context = Arc::new(Mutex::new(srtp_context));

        let local_to_remote_rtp_tx = self.start_rtp_sender(&rtcp_handler, event_tx.clone(), srtp_context.clone(), is_client)?;
        let remote_to_local_rtp_rx = self.start_rtp_receiver(&rtcp_handler, event_tx, srtp_context.clone(), is_client)?;

        Ok((local_to_remote_rtp_tx, remote_to_local_rtp_rx))
    }

    fn start_rtp_sender(
        &self,
        rtcp_handler: &Arc<Mutex<RtcpReportHandler<UdpSocket>>>,
        event_tx: Sender<AppEvent>,
        srtp_context: Arc<Mutex<SrtpContext>>,
        is_client: bool,
    ) -> Result<Sender<RtpPacket>, Error> {
        let rtp_sender_socket = self
            .rtp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;

        let rtp_sender = RtpSender::new(
            rtp_sender_socket,
            rtcp_handler,
            &self.connected,
            srtp_context,
            is_client,
        )
        .map_err(|e| Error::MapError(e.to_string()))?;

        rtp_sender
            .start(event_tx)
            .map_err(|e| Error::MapError(e.to_string()))
    }

    fn start_rtp_receiver(
        &self,
        rtcp_handler: &Arc<Mutex<RtcpReportHandler<UdpSocket>>>,
        event_tx: Sender<AppEvent>,
        srtp_context: Arc<Mutex<SrtpContext>>,
        is_client: bool,
    ) -> Result<Receiver<RtpPacket>, Error> {
        let rtp_receiver_socket = self
            .rtp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;

        let mut rtp_receiver = RtpReceiver::new(
            &self.config,
            rtp_receiver_socket,
            rtcp_handler,
            &self.connected,
            srtp_context,
            is_client
        )
        .map_err(|e| Error::MapError(e.to_string()))?;

        rtp_receiver
            .start(event_tx)
            .map_err(|e| Error::MapError(e.to_string()))
    }

    fn create_dtls_socket(
        &self,
        socket: UdpSocket,
        remote_addr: SocketAddr,
        local_setup_role: DtlsSetupRole,
        expected_fingerprint: Fingerprint,
        local_cert: &LocalCert
    ) -> Result<DtlsSocket, Error> {
        // Normalize roles
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
                // Prepare Identity
                let identity = local_cert.duplicate_identity()
                    .map_err(|e| Error::MapError(e.to_string()))?;

                // Use the crate's builder with Method Chaining
                let connector = DtlsConnector::builder()
                    .identity(identity)
                    .danger_accept_invalid_certs(true)
                    .danger_accept_invalid_hostnames(true)
                    .build()
                    .map_err(|e| Error::SocketConnectionError(e.to_string()))?;

                // Connect
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
                // Wrap manual acceptor in DtlsAcceptor tuple struct
                let acceptor = DtlsAcceptor(ssl_acceptor);

                acceptor
                    .accept(channel)
                    .map_err(|e| Error::SocketConnectionError(format!("{e:?}")))?
            }
            _ => unreachable!("DTLS role should be normalized before handshake"),
        };

        // Final security check: Verify the certificate matches the signaled fingerprint
        self.verify_peer_fingerprint(&stream, &expected_fingerprint)?;

        Ok(DtlsSocket::new(stream, remote_addr))
    }

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