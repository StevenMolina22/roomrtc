use std::fmt::Display;

#[derive(Debug)]
pub enum SCTPTransportError {
    ConnectError(String),
    OpenStreamError(String),
    OpenDataChannelError(String),
    IOError(String),
    PoisonedLock(String),
    SocketConfigError(String),
    MapError(String),
}

impl Display for SCTPTransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::ConnectError(e) => write!(f, "Error: \"Failed to connect\". Details: {e}"),
            Self::OpenStreamError(e) => write!(f, "Error: \"Failed to open stream\". Details: {e}"),
            Self::OpenDataChannelError(e) => write!(f, "{e}"),
            Self::IOError(e) => write!(f, "Error: \"I/O operation failed\". Details: {e}"),
            Self::PoisonedLock(e) => write!(f, "Error: \"Poisoned lock\" . Details: {e}"),
            Self::SocketConfigError(e) => {
                write!(f, "Error: \"Failed to config socket\" . Details: {e}")
            }
            Self::MapError(e) => write!(f, "{e}"),
        }
    }
}
