use std::fmt::Display;

pub enum RTCPPackage {
    ConnectivityReport,
    Goodbye,
}

impl Display for RTCPPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            RTCPPackage::ConnectivityReport => write!(f, "CR"),
            RTCPPackage::Goodbye => write!(f, "BYE"),
        }
    }
}

impl RTCPPackage {
    pub fn as_bytes(&self) -> &'static [u8] {
        match self {
            RTCPPackage::ConnectivityReport => b"CR",
            RTCPPackage::Goodbye => b"BY",
        }
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        match data {
            b"CR" => Some(RTCPPackage::ConnectivityReport),
            b"BY" => Some(RTCPPackage::Goodbye),
            _ => None,
        }
    }
}
