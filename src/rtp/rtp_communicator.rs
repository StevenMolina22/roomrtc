use std::net::{UdpSocket, SocketAddr};
use std::io::{Error, ErrorKind};
use crate::rtp::rtp_package::RtpPackage;

pub struct RtpSender {
    socket: UdpSocket,
    sequence_number: u16,
    ssrc: u32,
}

fn get_local_ip() -> Result<String, Box<dyn std::error::Error>> {
    let interfaces = if_addrs::get_if_addrs().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    for interface in interfaces {
        if !interface.is_loopback() {
            return Ok(interface.addr.ip().to_string());
        }
    }
    Err(Box::new(std::io::Error::new(ErrorKind::NotFound, "no network interface found")))
}

impl RtpSender {
    pub fn new(dest: SocketAddr, ssrc: u32) -> Result<Self, Error> {
        let local_ip = get_local_ip().map_err(|e| Error::new(ErrorKind::Other, format!("get_local_ip: {}", e)))?;
        let local_addr: SocketAddr = format!("{}:0", local_ip).parse().map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;
        let socket = UdpSocket::bind(local_addr).map_err(|e| Error::new(ErrorKind::AddrNotAvailable, e))?;
        socket.connect(dest).map_err(|e| Error::new(ErrorKind::AddrNotAvailable, e))?;

        Ok(Self {
            socket,
            sequence_number: 0,
            ssrc,
        })
    }

    pub fn send(&mut self, payload: &[u8], payload_type: u8, timestamp: u32, marker: bool) -> Result<(), Error> {
        let rtp_package = RtpPackage::new(
            marker,
            payload_type,
            payload.to_vec(),
            timestamp,
            self.sequence_number,
            self.ssrc,
        );

    let data = rtp_package.to_bytes();
    self.socket.send(&data).map_err(|e| Error::new(ErrorKind::Other, e))?;

        self.sequence_number = self.sequence_number.wrapping_add(1);

        Ok(())
    }
    
}

pub struct RtpReceiver {
    socket: UdpSocket,
}

impl RtpReceiver {
    /// Crea un receptor RTP enlazado a la IP local y puerto dado
    pub fn new(bind_port: u16) -> Result<Self, Error> {
        // no anduvo con esto no se por que
        // let local_ip = get_local_ip().map_err(|e| Error::new(ErrorKind::Other, format!("get_local_ip: {}", e)))?;
        // let bind_addr = format!("{}:{}", local_ip, bind_port);
        
        // Bind to 0.0.0.0 so we accept packets on any local interface (including loopback)
        let bind_addr = format!("0.0.0.0:{}", bind_port);
        let socket = UdpSocket::bind(bind_addr).map_err(|e| Error::new(ErrorKind::AddrNotAvailable, e))?;
        socket.set_nonblocking(true).map_err(|e| Error::new(ErrorKind::Other, e))?;

        Ok(Self { socket })
    }

    /// Intenta recibir un paquete RTP. Devuelve Some(RtpPackage) si llegó un paquete, None si no hay datos
    pub fn try_receive(&self) -> Result<Option<RtpPackage>, Error> {
        let mut buf = [0u8; 1500]; // MTU típico de red
        match self.socket.recv(&mut buf) {
            Ok(size) => {
                if let Some(pkg) = RtpPackage::from_bytes(&buf[..size]) {
                    Ok(Some(pkg))
                } else {
                    Err(Error::new(ErrorKind::InvalidData, "RTP parse error"))
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}
