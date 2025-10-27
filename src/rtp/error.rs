use std::fmt::{Display, Formatter};

/// Represents the different types of errors that can occur while handling RTP operations.
#[derive(PartialEq, Eq, Debug)]
pub enum RTPError {
    /// The requested address is not available.
    AddrNotAvailable,
    /// Failed to acquire a lock due to a concurrent thread error.
    PoisonedLock,
    /// Failed to bind or connect a UDP socket in non-blocking mode.
    BlockingSocket,
    /// Failed to clone a socket.
    SocketCloneFailed,
    /// Failed to send through the socket.
    SendFailed,
    /// Failed to receive from the socket.
    ReceiveFailed,
    /// The received RTP packet was invalid, malformed, or corrupted.
    InvalidRtpPacket,
    /// Failed to terminate an active RTP connection or related thread.
    TerminateFailed,
    /// RTCP Error
    RTCPError(String),
}

impl Display for RTPError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RTPError::AddrNotAvailable => write!(f, "Error: \"Address not available\""),
            RTPError::BlockingSocket => {
                write!(f, "Error: \"Failed to bind or connect UDP socket\"")
            }
            RTPError::PoisonedLock => write!(f, "Error: \"Poisoned lock\""),
            RTPError::SendFailed => write!(f, "Error: \"Failed to send RTP packet\""),
            RTPError::ReceiveFailed => write!(f, "Error: \"Failed to receive RTP packet\""),
            RTPError::InvalidRtpPacket => write!(f, "Error: \"Invalid or corrupted RTP packet\""),
            RTPError::SocketCloneFailed => write!(f, "Error: \"Failed to clone socket\""),
            RTPError::TerminateFailed => write!(f, "Error: \"Failed to terminate\""),
            RTPError::RTCPError(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for RTPError {}
