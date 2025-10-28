use std::net::UdpSocket;
use std::time::Duration;

pub trait Socket {
    fn send(&self, buf: &[u8]) -> Result<usize, std::io::Error>;
    fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, std::net::SocketAddr), std::io::Error>;
    fn set_read_timeout(&self, dur: Option<Duration>) -> Result<(), std::io::Error>;
}

impl Socket for UdpSocket {
    fn send(&self, buf: &[u8]) -> Result<usize, std::io::Error> {
        UdpSocket::send(self, buf)
    }
    fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, std::net::SocketAddr), std::io::Error> {
        UdpSocket::recv_from(self, buf)
    }
    fn set_read_timeout(&self, dur: Option<Duration>) -> Result<(), std::io::Error> {
        UdpSocket::set_read_timeout(self, dur)
    }
}
