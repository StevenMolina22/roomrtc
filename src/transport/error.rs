use std::fmt::Display;

pub enum MediaTransportError {
    BindingError(String),
    SocketConnectionError(String),
    MapError(String),
    CloningSocketError(String),
    ConnectionNotStarted,
    ConnectionError(String),
}

impl Display for MediaTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::BindingError(e) => write!(f, "Error: \"Socket binding failed: {}\"", e),
            Self::SocketConnectionError(e) => write!(f, "Error: \"Socket connection failed\": {}", e),
            Self::MapError(e) => write!(f, "{}", e),
            Self::CloningSocketError(e) => write!(f, "Error: \"Socket clone failed\": {}", e),
            Self::ConnectionNotStarted => write!(f, "Error: \"Connection not started yet\""),
        }
    }
}