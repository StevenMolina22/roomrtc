use std::fmt::Display;

#[derive(Debug)]
pub enum MediaPipelineError {
    MapError(String),
    ParsingError(String),
    SendError(String),
    ProtectionError(String),
    PoisonedLock,
}

impl Display for MediaPipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::MapError(e) => write!(f, "{e}"),
            Self::ParsingError(e) => write!(f, "Error: \"Parsing failed\": {e}"),
            Self::SendError(e) => write!(f, "Error: \"Send failed\": {e}"),
            Self::ProtectionError(e) => write!(f, "Error: \"Failed to protect RTP packet ({e})"),
            Self::PoisonedLock => write!(f, "Error: \"Poisoned lock\""),
        }
    }
}
