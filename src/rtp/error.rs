use std::fmt::{Display, Formatter};

/// Represents the different types of errors that can occur while handling RTP operations.
#[derive(PartialEq, Eq, Debug)]
pub enum RtpError {
    /// The requested address is not available.
    AddrNotAvailable,
    /// Failed to configure socket.
    SocketConfigFailed,
    /// Failed to clone a socket.
    SocketCloneFailed,
    /// Failed to send through the socket.
    SendFailed,
    /// Failed to receive from the socket.
    ReceiveFailed(String),
    /// The received RTP packet was invalid, malformed, or corrupted.
    InvalidRtpPacket,
    /// Failed to terminate an active RTP connection or related thread.
    TerminateFailed,
    /// Failed to send or receive because connection has been closed
    ConnectionClosed,
    /// Failed to acquire connection status lock
    ConnectionStatusLockFailed,
    /// RTCP Error
    RTCPError(String),
    /// Poisoned lock
    PoisonedLock,
}

impl Display for RtpError {
    /// Format the error as a short human-readable message.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AddrNotAvailable => write!(f, "Error: \"Address not available\""),
            Self::SocketConfigFailed => write!(f, "Error: \"Failed to configure socket\""),
            Self::SendFailed => write!(f, "Error: \"Failed to send RTP packet\""),
            Self::ReceiveFailed(e) => {
                write!(f, "Error: \"Failed to receive RTP packet. Details: {e}\"")
            }
            Self::InvalidRtpPacket => write!(f, "Error: \"Invalid or corrupted RTP packet\""),
            Self::SocketCloneFailed => write!(f, "Error: \"Failed to clone socket\""),
            Self::TerminateFailed => write!(f, "Error: \"Failed to terminate\""),
            Self::ConnectionClosed => write!(f, "Error: \"Connection closed\""),
            Self::RTCPError(e) => write!(f, "{e}"),
            Self::ConnectionStatusLockFailed => {
                write!(f, "Error: \"Failed to acquire connection status lock\"")
            }
            Self::PoisonedLock => write!(f, "Error: \"The mutex was poisoned\""),
        }
    }
}

impl std::error::Error for RtpError {}
