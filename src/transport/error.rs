use std::fmt::Display;

/// Errors that can occur during media transport operations.
///
/// This enum covers errors from socket binding, DTLS handshake, SRTP setup,
/// and runtime failures in the RTP/RTCP pipeline.
#[derive(Debug)]
pub enum MediaTransportError {
    /// Socket binding failed (e.g., port already in use, invalid address).
    BindingError(String),
    /// Socket connection to remote address failed.
    SocketConnectionError(String),
    /// Generic error with custom message (DTLS, SRTP, protocol errors).
    MapError(String),
    /// Failed to clone a socket for multi-threaded use.
    CloningSocketError(String),
    /// Attempted to use transport before calling `start()`.
    ConnectionNotStarted,
    /// Failed to configure socket options (e.g., timeouts).
    SocketConfigFailed,
    /// SRTP encryption or decryption failed.
    ProtectionError(String),
    /// Failed to send data through an internal channel.
    ChannelSendError(String),
}

impl Display for MediaTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::BindingError(e) => write!(f, "Error: \"Socket binding failed: {e}\""),
            Self::SocketConnectionError(e) => {
                write!(f, "Error: \"Socket connection failed\": {e}")
            }
            Self::MapError(e) => write!(f, "{e}"),
            Self::CloningSocketError(e) => write!(f, "Error: \"Socket clone failed\": {e}"),
            Self::ConnectionNotStarted => write!(f, "Error: \"Connection not started yet\""),
            Self::SocketConfigFailed => write!(f, "Error: \"Failed to configure socket\""),
            Self::ProtectionError(e) => write!(f, "Error: \"Protection failed: {e}\""),
            Self::ChannelSendError(e) => write!(f, "Error: \"Failed to send through channel: {e}\""),
        }
    }
}
