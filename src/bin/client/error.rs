use std::fmt::Display;

#[derive(Eq, PartialEq, Debug)]
pub enum ClientError {
    SdpCreationError(String),
    IceConnectionError(String),
}

impl Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            ClientError::SdpCreationError(e) => write!(f, "Error: \"could not create SDP\". Details: {}", e),
            ClientError::IceConnectionError(e) => write!(f, "Error: \"could not connect to ice connection\". Details: {}", e),
        }
    }
}