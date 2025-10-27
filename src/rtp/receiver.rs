use crate::rtp::error::RTPError;
use crate::rtp::rtp_package::RtpPackage;
use std::net::UdpSocket;

pub struct RtpReceiver {
    socket: UdpSocket,
    last_sequence_number: Option<u16>,
}

impl RtpReceiver {
    /// Creates an RTP receptor bound to the local IP at the given port
    pub fn new(bind_port: u16) -> Result<Self, RTPError> {
        let bind_addr = format!("0.0.0.0:{}", bind_port);
        let socket = UdpSocket::bind(bind_addr).map_err(|_| RTPError::AddrNotAvailable)?;
        socket
            .set_nonblocking(true)
            .map_err(|_| RTPError::BlockingSocket)?;

        Ok(Self {
            socket,
            last_sequence_number: None,
        })
    }

    /// Attempts to receive an RTP packet. Returns Some(RtpPackage) if a packet was received, or None if no data is available.
    pub fn try_receive(&mut self) -> Result<Option<RtpPackage>, RTPError> {
        let mut buf = [0u8; 1500];
        match self.socket.recv(&mut buf) {
            Ok(size) => {
                if let Some(pkg) = RtpPackage::from_bytes(&buf[..size]) {
                    if let Some(last_seq) = self.last_sequence_number {
                        let expected = last_seq.wrapping_add(1);
                        if pkg.sequence_number != expected {
                            //Generate RTCP to notify a package loss
                        }
                    }
                    self.last_sequence_number = Some(pkg.sequence_number);
                    Ok(Some(pkg))
                } else {
                    Ok(None)
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(_) => Err(RTPError::ReceiveFailed),
        }
    }
}
