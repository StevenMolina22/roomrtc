use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use udp_dtls::{DtlsStream, UdpChannel};

use crate::tools::Socket;

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
    /// Creates a new `DtlsSocket` from an established DTLS stream.
    #[must_use]
    pub fn new(stream: DtlsStream<UdpChannel>, remote_addr: SocketAddr) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
            remote_addr,
        }
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
