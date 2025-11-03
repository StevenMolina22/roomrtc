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
    SendFailed(String),
    ReceiveFailed(String),
}

impl Display for RtcpError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PoisonedLock => write!(f, "Error: Poisoned lock"),
            Self::InvalidRTCPPacket => {
                write!(f, "Error: \"Invalid or corrupted RTCP packet\"")
            }
            Self::SocketCloneFailed => write!(f, "Error: \"Failed to clone socket\""),
            Self::SocketConfigFailed => write!(f, "Error: \"Failed to configure socket\""),
            Self::GoodbyeReceived => write!(f, "Error: \"Goodbye\""),
            Self::TimedOut => write!(f, "Error: \"Report receiver timed out\""),
            Self::ConnectionStatusLockFailed => {
                write!(f, "Error: \"Failed to acquire connection status lock\"")
            }
            Self::InvalidConfigDuration => {
                write!(f, "Error: \"Invalid configuration duration\"")
            }
            Self::SendFailed(e) => write!(f, "Error: \"Failed to send RTCP packet. Details: {}\"", e),
            Self::ReceiveFailed(e) => write!(f, "Error: \"Failed to receive RTCP packet. Details: {}\"", e),
        }
    }
}

impl std::error::Error for RtcpError {}
