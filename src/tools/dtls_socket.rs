use super::socket::Socket;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use udp_dtls::{DtlsStream, UdpChannel};

/// Socket implementation backed by a DTLS stream.
///
/// This wrapper shares the underlying DTLS session using an `Arc<Mutex<_>>`
/// so RTP sender/receiver threads can share the same secure channel.
#[derive(Clone)]
pub struct DtlsSocket {
    stream: Arc<Mutex<DtlsStream<UdpChannel>>>,
    remote_addr: SocketAddr,
}

impl DtlsSocket {
    /// Create a new DTLS socket from the established stream.
    pub fn new(stream: DtlsStream<UdpChannel>, remote_addr: SocketAddr) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
            remote_addr,
        }
    }

    fn with_stream<T>(
        &self,
        f: impl FnOnce(&mut DtlsStream<UdpChannel>) -> io::Result<T>,
    ) -> io::Result<T> {
        let mut guard = self
            .stream
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "DTLS stream poisoned"))?;
        f(&mut guard)
    }

    pub fn export_keying_material(&self, label: &str, len: usize) -> io::Result<Vec<u8>> {
        self.with_stream(|stream| {
            let mut buf = vec![0u8; len];
            stream
                .0
                .ssl()
                .export_keying_material(&mut buf, label, None)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
            Ok(buf)
        })
    }
}

impl Socket for DtlsSocket {
    fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.with_stream(|stream| stream.write(buf))
    }

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        let size = self.with_stream(|stream| stream.read(buf))?;
        Ok((size, self.remote_addr))
    }

    fn set_read_timeout(&self, dur: Option<Duration>) -> io::Result<()> {
        let guard = self
            .stream
            .lock()
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "DTLS stream poisoned"))?;
        guard.get_ref().socket.set_read_timeout(dur)
    }
}
