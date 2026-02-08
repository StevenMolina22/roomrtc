use std::fmt::Display;

#[derive(Debug)]
pub enum FileTransfererError {
    MapError(String),
    FileReadError(String),
    FileCreateError(String),
    UnexpectedIncomingMessage,
    FileWriteError(String),
    ChannelSendError(String),
    UnknownIncomingMessage,
    LockError(String),
    UnknownOfferId(u32),
}

impl Display for FileTransfererError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::MapError(e) => write!(f, "{}", e),
            Self::FileReadError(e) => {
                write!(f, "Error: \"Error while reading a file\". Details: {}", e)
            }
            Self::FileWriteError(e) => {
                write!(f, "Error: \"Error while writing a file\". Details: {}", e)
            }
            Self::FileCreateError(e) => {
                write!(f, "Error: \"Error while creating a file\". Details: {}", e)
            }
            Self::ChannelSendError(e) => {
                write!(f, "Error: \"DataChannel send failed\". Details: {}", e)
            }
            Self::UnexpectedIncomingMessage => write!(f, "Error: \"Unexpected Incoming Message\""),
            Self::UnknownIncomingMessage => write!(f, "Error: \"Unknown Incoming Message\""),
            Self::LockError(e) => write!(f, "Error: \"Lock error\". Details: {}", e),
            Self::UnknownOfferId(id) => write!(f, "Error: \"Unknown offer id\". Details: {}", id),
        }
    }
}
