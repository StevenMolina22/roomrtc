use std::fmt::{Display, Formatter};

#[derive(PartialEq, Eq, Debug)]
pub enum RTCPError {
    PoisonedLock,
    SocketCloneFailed,
    InvalidRTCPPacket,
    SocketConfigFailed,
    GoodbyeReceived, 
    TimedOut,
}

impl Display for RTCPError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RTCPError::PoisonedLock => write!(f, "Error: Poisoned lock"),
            RTCPError::InvalidRTCPPacket => write!(f, "Error: \"Invalid or corrupted RTCP packet\""),
            RTCPError::SocketCloneFailed => write!(f, "Error: \"Failed to clone socket\""),
            RTCPError::SocketConfigFailed => write!(f, "Error: \"Failed to configure socket\""),
            RTCPError::GoodbyeReceived => write!(f, "Error: \"Goodbye\""),
            RTCPError::TimedOut => write!(f, "Error: \"Report receiver timed out\""),
        }
    }
}

impl std::error::Error for RTCPError {}
