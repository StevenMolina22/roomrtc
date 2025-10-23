use crate::rtp::error::RTPError;
use crate::rtp::rtp_package::RtpPackage;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};

pub struct RtpSender {
    socket: UdpSocket,
    sequence_number: u16,
    ssrc: u32,
}

fn get_local_ip() -> Result<String, Box<dyn std::error::Error>> {
    let interfaces =
        if_addrs::get_if_addrs().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    for interface in interfaces {
        if !interface.is_loopback() {
            return Ok(interface.addr.ip().to_string());
        }
    }
    Err(Box::new(std::io::Error::new(
        ErrorKind::NotFound,
        "no network interface found",
    )))
}

impl RtpSender {
    pub fn new(dest: SocketAddr, ssrc: u32) -> Result<Self, RTPError> {
        let local_ip = get_local_ip().map_err(|_| RTPError::InvalidAddr)?;
        let local_addr: SocketAddr = format!("{}:0", local_ip)
            .parse()
            .map_err(|_| RTPError::InvalidAddr)?;
        let socket = UdpSocket::bind(local_addr).map_err(|_| RTPError::AddrNotAvailable)?;
        socket
            .connect(dest)
            .map_err(|_| RTPError::AddrNotAvailable)?;

        Ok(Self {
            socket,
            sequence_number: 0,
            ssrc,
        })
    }

    pub fn send(
        &mut self,
        payload: &[u8],
        payload_type: u8,
        timestamp: u32,
        marker: bool,
    ) -> Result<(), RTPError> {
        let rtp_package = RtpPackage::new(
            marker,
            payload_type,
            payload.to_vec(),
            timestamp,
            self.sequence_number,
            self.ssrc,
        );

        let data = rtp_package.to_bytes();
        self.socket.send(&data).map_err(|_| RTPError::SendFailed)?;

        self.sequence_number = self.sequence_number.wrapping_add(1);

        Ok(())
    }
}

pub struct RtpReceiver {
    socket: UdpSocket,
    last_sequence_number: Option<u16>,
}

impl RtpReceiver {
    /// Crea un receptor RTP enlazado a la IP local y puerto dado
    pub fn new(bind_port: u16) -> Result<Self, RTPError> {
        // no anduvo con esto no se por que
        // let local_ip = get_local_ip().map_err(|e| Error::new(ErrorKind::Other, format!("get_local_ip: {}", e)))?;
        // let bind_addr = format!("{}:{}", local_ip, bind_port);

        // Bind to 0.0.0.0 so we accept packets on any local interface (including loopback)
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

    /// Intenta recibir un paquete RTP. Devuelve Some(RtpPackage) si llegó un paquete, None si no hay datos
    pub fn try_receive(&mut self) -> Result<Option<RtpPackage>, RTPError> {
        let mut buf = [0u8; 1500]; // MTU típico de red
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
                    Err(RTPError::InvalidRtpPacket)
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(_) => Err(RTPError::ReceiveFailed),
        }
    }
}
