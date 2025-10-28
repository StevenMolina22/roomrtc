use std::fmt::Display;

pub enum RtcpPacket {
    ConnectivityReport,
    Goodbye,
}

impl Display for RtcpPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            RtcpPacket::ConnectivityReport => write!(f, "CR"),
            RtcpPacket::Goodbye => write!(f, "BYE"),
        }
    }
}

impl RtcpPacket {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            RtcpPacket::ConnectivityReport => b"CR",
            RtcpPacket::Goodbye => b"BY",
        }
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        match data {
            b"CR" => Some(RtcpPacket::ConnectivityReport),
            b"BY" => Some(RtcpPacket::Goodbye),
            _ => None,
        }
    }
}
