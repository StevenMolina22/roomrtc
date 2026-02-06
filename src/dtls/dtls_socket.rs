use super::DtlsError as Error;
use crate::dtls::key_manager::{LocalCert, PKCS12_PASSWORD};
use crate::session::sdp::{DtlsSetupRole, Fingerprint};
use crate::tools::Socket;
use openssl::pkcs12::Pkcs12;
use openssl::ssl::{SslAcceptor, SslMethod, SslVerifyMode};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use udp_dtls::{DtlsAcceptor, DtlsConnector, DtlsStream, SignatureAlgorithm, UdpChannel};

/// A thread-safe wrapper around a DTLS stream that implements the `Socket` trait.
///
/// The DTLS socket allows the application to treat an encrypted DTLS connection
/// like a standard UDP socket. It handles the underlying locking mechanism
/// (`Arc<Mutex>`) needed to share the single secure connection between independent
/// `RtpSender` and `RtpReceiver` threads.
#[derive(Clone)]
pub struct DtlsSocket {
    /// The shared, encrypted stream. Protected by a Mutex to allow concurrent access
    /// (e.g., sending heartbeat packets while listening for media).
    stream: Arc<Mutex<DtlsStream<UdpChannel>>>,

    /// The address of the remote peer. Stored here to satisfy the `recv_from` signature.
    remote_addr: SocketAddr,
}

impl DtlsSocket {
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
    pub fn new(
        socket: UdpSocket,
        remote_addr: SocketAddr,
        local_setup_role: DtlsSetupRole,
        expected_fingerprint: Fingerprint,
        local_cert: &LocalCert,
    ) -> Result<Self, Error> {
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
                    .map_err(|e| Error::InitializationSocketError)?;

                connector
                    .connect("roomrtc.local", channel)
                    .map_err(|e| Error::InitializationSocketError)?
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
                    .map_err(|e| Error::InitializationSocketError)?
            }
            _ => unreachable!("DTLS role should be normalized before handshake"),
        };

        verify_peer_fingerprint(&stream, &expected_fingerprint)?;

        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
            remote_addr,
        })
    }

    /// Internal helper to acquire the lock on the DTLS stream.
    ///
    /// This handles the `Mutex` locking and converts poisoning errors into
    /// `std::io::Error`, simplifying error handling in `send`/`recv`.
    fn with_stream<T>(
        &self,
        f: impl FnOnce(&mut DtlsStream<UdpChannel>) -> io::Result<T>,
    ) -> io::Result<T> {
        let mut guard = self
            .stream
            .lock()
            .map_err(|_| io::Error::other("DTLS stream poisoned"))?;
        f(&mut guard)
    }

    /// Exports keying material (RFC 5764) for SRTP key derivation.
    ///
    /// In WebRTC, DTLS is used not just for the handshake, but to generate
    /// the secrets used by SRTP. This method extracts those secrets so the
    /// `SrtpContext` can be initialized for media encryption.
    pub fn export_keying_material(&self, label: &str, len: usize) -> io::Result<Vec<u8>> {
        self.with_stream(|stream| {
            let mut buf = vec![0u8; len];
            stream
                .0
                .ssl()
                .export_keying_material(&mut buf, label, None)
                .map_err(|e| io::Error::other(e.to_string()))?;
            Ok(buf)
        })
    }
}

impl Socket for DtlsSocket {
    /// Encrypts the buffer via DTLS and sends it to the remote peer.
    fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.with_stream(|stream| stream.write(buf))
    }

    /// Receives data from the socket, decrypting it via DTLS.
    ///
    /// Returns the number of bytes read and the stored `remote_addr`.
    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let size = self.with_stream(|stream| stream.read(buf))?;
        Ok((size, self.remote_addr))
    }

    /// Sets the read timeout for the underlying UDP socket.
    fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        let guard = self
            .stream
            .lock()
            .map_err(|_| io::Error::other("DTLS stream poisoned"))?;
        guard.get_ref().socket.set_read_timeout(dur)
    }

    /// Returns a new handle pointing to the same DTLS stream.
    /// The underlying DTLS session is shared via `Arc<Mutex<...>>`.
    fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            stream: Arc::clone(&self.stream),
            remote_addr: self.remote_addr,
        })
    }
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
