use std::fmt::Display;
use std::net::AddrParseError;

#[derive(Debug, Eq, PartialEq)]
pub enum ControllerError {
    ParsingSocketAddressError(AddrParseError),
    BindingAddressError(String),
    ConnectionSocketError(String),
    CloningSocketError(String),
    RtpSenderError(String),
    RtpReceiverError(String),
    PoisonedLock,
}

impl Display for ControllerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParsingSocketAddressError(e) => write!(f, "Error: \"Failed to parse socket address\". Details: {}", e),
            Self::BindingAddressError(e) => write!(f, "Error: \"Failed to bind socket\". Details: {}", e),
            Self::ConnectionSocketError(e) => write!(f, "Error: \"Failed to connect\". Details: {}", e),
            Self::CloningSocketError(e) => write!(f, "Error: \"Failed to clone UDP socket\" Details: {}", e),
            Self::RtpSenderError(e) => write!(f, "Error: \"Failed to create RTP sender. Details: {}", e),
            Self::RtpReceiverError(e) => write!(f, "Error: \"Failed to create RTP receiver. Details: {}", e),
            Self::PoisonedLock => write!(f, "Error: \"Poisoned lock\""),
        }
    }
}