use std::fmt::Display;

#[derive(Debug)]
pub enum ClientError {
    SdpCreationError,
    IceConnectionError,
}

impl Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            ClientError::SdpCreationError => write!(f, "Error: \"could not create SDP\""),
            ClientError::IceConnectionError => write!(f, "Error: \"could not connect to ice connection\""),
        }
    }
}