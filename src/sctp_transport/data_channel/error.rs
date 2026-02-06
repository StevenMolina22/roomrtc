use std::fmt::Display;

#[derive(Debug)]
pub enum DataChannelError {
    OpenError(String),
    ReadStreamError(String),
    OpenTimeout,
    SendError(String),
}

impl Display for DataChannelError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::OpenError(e) => write!(f, "Error: \"Failed to open data channel\". Details: {e}"),
            Self::OpenTimeout => {
                write!(
                    f,
                    "Error: \"Failed to open data channel\". Details: Timed out waiting for acknowledgement"
                )
            }
            Self::ReadStreamError(e) => {
                write!(
                    f,
                    "Error: \"Failed to read from data channel\". Details: {e}"
                )
            }
            Self::SendError(e) => {
                write!(f, "Error: \"Failed to send to data channel\". Details: {e}")
            }
        }
    }
}
