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
}

impl Display for RtpError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RtpError::AddrNotAvailable => write!(f, "Error: \"Address not available\""),
            RtpError::SocketConfigFailed => write!(f, "Error: \"Failed to configure socket\""),
            RtpError::SendFailed => write!(f, "Error: \"Failed to send RTP packet\""),
            RtpError::ReceiveFailed(e) => write!(f, "Error: \"Failed to receive RTP packet. Details: {}\"", e),
            RtpError::InvalidRtpPacket => write!(f, "Error: \"Invalid or corrupted RTP packet\""),
            RtpError::SocketCloneFailed => write!(f, "Error: \"Failed to clone socket\""),
            RtpError::TerminateFailed => write!(f, "Error: \"Failed to terminate\""),
            RtpError::ConnectionClosed => write!(f, "Error: \"Connection closed\""),
            RtpError::RTCPError(e) => write!(f, "{}", e),
            RtpError::ConnectionStatusLockFailed => write!(f, "Error: \"Failed to acquire connection status lock\""),
        }
    }
}

impl std::error::Error for RtpError {}
