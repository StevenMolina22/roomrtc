use std::fmt::Display;

#[derive(Debug)]
pub enum SCTPTransportError {
    ConnectError(String),
    OpenStreamError(String),
    OpenDataChannelError(String),
    IOError(String),
}

impl Display for SCTPTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::ConnectError(e) => write!(f, "Error: \"Failed to connect\". Details: {e}"),
            Self::OpenStreamError(e) => write!(f, "Error: \"Failed to open stream\". Details: {e}"),
            Self::OpenDataChannelError(e) => write!(f, "{e}"),
            Self::IOError(e) => write!(f, "Error: \"I/O operation failed\". Details: {e}"),
        }
    }
}
