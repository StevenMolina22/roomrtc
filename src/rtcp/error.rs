use std::fmt::{Display, Formatter};

#[derive(PartialEq, Eq, Debug)]
pub enum RtcpError {
    PoisonedLock,
    SocketCloneFailed,
    InvalidRTCPPacket,
    SocketConfigFailed,
    GoodbyeReceived,
    TimedOut,
    ConnectionStatusLockFailed,
    InvalidConfigDuration,
}

impl Display for RtcpError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RtcpError::PoisonedLock => write!(f, "Error: Poisoned lock"),
            RtcpError::ConnectionStatusLockFailed => {
                write!(f, "Error: \"Failed to acquire connection status lock\"")
            }
            RtcpError::InvalidRTCPPacket => {
                write!(f, "Error: \"Invalid or corrupted RTCP packet\"")
            }
            RtcpError::SocketCloneFailed => write!(f, "Error: \"Failed to clone socket\""),
            RtcpError::SocketConfigFailed => write!(f, "Error: \"Failed to configure socket\""),
            RtcpError::GoodbyeReceived => write!(f, "Error: \"Goodbye\""),
            RtcpError::TimedOut => write!(f, "Error: \"Report receiver timed out\""),
            RtcpError::InvalidConfigDuration => {
                write!(f, "Error: \"Invalid configuration duration\"")
            }
        }
    }
}

impl std::error::Error for RtcpError {}
