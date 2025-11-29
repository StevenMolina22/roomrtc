use std::fmt::Display;

#[derive(Debug)]
pub enum MediaTransportError {
    BindingError(String),
    SocketConnectionError(String),
    MapError(String),
    CloningSocketError(String),
    ConnectionNotStarted,

    /// Failed to configure the socket (timeout, options, etc.).
    SocketConfigFailed,
}

impl Display for MediaTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::BindingError(e) => write!(f, "Error: \"Socket binding failed: {}\"", e),
            Self::SocketConnectionError(e) => write!(f, "Error: \"Socket connection failed\": {}", e),
            Self::MapError(e) => write!(f, "{}", e),
            Self::CloningSocketError(e) => write!(f, "Error: \"Socket clone failed\": {}", e),
            Self::ConnectionNotStarted => write!(f, "Error: \"Connection not started yet\""),
            Self::SocketConfigFailed => write!(f, "Error: \"Failed to configure socket\""),
        }
    }
}