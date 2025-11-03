use std::fmt::Display;

pub enum RtcpPacket {
    ConnectivityReport,
    Goodbye,
    Hello,
    Ready,
}

impl Display for RtcpPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            RtcpPacket::ConnectivityReport => write!(f, "CR"),
            RtcpPacket::Goodbye => write!(f, "BYE"),
            RtcpPacket::Hello => write!(f, "HELLO"),
            RtcpPacket::Ready => write!(f, "READY"),
        }
    }
}

impl RtcpPacket {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            RtcpPacket::ConnectivityReport => b"CR",
            RtcpPacket::Goodbye => b"BY",
            RtcpPacket::Hello => b"HELLO",
            RtcpPacket::Ready => b"READY",
        }
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        match data {
            b"CR" => Some(RtcpPacket::ConnectivityReport),
            b"BY" => Some(RtcpPacket::Goodbye),
            b"HELLO" => Some(RtcpPacket::Hello),
            b"READY" => Some(RtcpPacket::Ready),
            _ => None,
        }
    }
}
