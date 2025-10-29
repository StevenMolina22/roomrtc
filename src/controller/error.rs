use std::fmt::Display;

#[derive(Debug, Eq, PartialEq)]
pub enum ControllerError {
    ParsingSocketAddressError(String),
    BindingAddressError(String),
    ConnectionSocketError(String),
    CloningSocketError(String),
}

impl Display for ControllerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParsingSocketAddressError(e) => write!(f, "Error: \"Failed to parse socket address\". Details: {}", e),
            Self::BindingAddressError(e) => write!(f, "Error: \"Failed to bind socket\". Details: {}", e),
            Self::ConnectionSocketError(e) => write!(f, "Error: \"Failed to connect\". Details: {}", e),
            Self::CloningSocketError(e) => write!(f, "Error: \"Failed to clone UDP socket\" Details: {}", e),
        }
    }
}