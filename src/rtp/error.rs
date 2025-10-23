use std::fmt::{Display, Formatter};

#[derive(PartialEq, Eq, Debug)]
pub enum RTPError {
    InvalidAddr,
    AddrNotAvailable,
    BlockingSocket,
    SendFailed,
    ReceiveFailed,
    InvalidRtpPacket,
}

impl Display for RTPError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RTPError::InvalidAddr => write!(f, "Error: \"Invalid address\""),
            RTPError::AddrNotAvailable => write!(f, "Error: \"Address not available\""),
            RTPError::BlockingSocket => {
                write!(f, "Error: \"Failed to bind or connect UDP socket\"")
            }
            RTPError::SendFailed => write!(f, "Error: \"Failed to send RTP packet\""),
            RTPError::ReceiveFailed => write!(f, "Error: \"Failed to receive RTP packet\""),
            RTPError::InvalidRtpPacket => write!(f, "Error: \"Invalid or corrupted RTP packet\""),
        }
    }
}

impl std::error::Error for RTPError {}
