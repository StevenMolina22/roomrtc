use std::net::UdpSocket;
use std::time::Duration;

/// Abstract socket operations.
pub trait Socket {
    /// Send bytes to a remote peer.
    ///
    /// Returns the number of bytes written on success or an `std::io::Error`.
    fn send(&self, buf: &[u8]) -> Result<usize, std::io::Error>;

    /// Receive a datagram, writing into `buf` and returning the number
    /// of bytes received along with the sender address.
    ///
    /// Implementations should follow the same semantics as
    /// `std::net::UdpSocket::recv_from`.
    fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, std::net::SocketAddr), std::io::Error>;

    /// Configure the read timeout for the socket.
    ///
    /// Passing `None` clears the timeout. Returns an `std::io::Error` on
    /// failure.
    fn set_read_timeout(&self, dur: Option<Duration>) -> Result<(), std::io::Error>;

    fn try_clone(&self) -> Result<Self, std::io::Error> where Self: Sized;
}

/// Provide the `Socket` trait for the standard library `UdpSocket` so
/// production code can use `UdpSocket` directly without changing the
/// networking logic.
impl Socket for UdpSocket {
    fn send(&self, buf: &[u8]) -> Result<usize, std::io::Error> {
        Self::send(self, buf)
    }

    fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, std::net::SocketAddr), std::io::Error> {
        Self::recv_from(self, buf)
    }

    fn set_read_timeout(&self, dur: Option<Duration>) -> Result<(), std::io::Error> {
        Self::set_read_timeout(self, dur)
    }

    fn try_clone(&self) -> Result<Self, std::io::Error> {
        Self::try_clone(self)
    }
}
