use std::fmt::Display;

/// Errors that can occur within the media pipeline.
///
/// These cover configuration/mapping failures, parsing issues, send errors,
/// SRTP protection failures, and poisoned synchronization primitives.
#[derive(Debug)]
pub enum MediaPipelineError {
    /// Generic mapping or configuration error with details.
    MapError(String),
    /// Parsing failure with context.
    ParsingError(String),
    /// Failure when sending data (e.g., across channels or sockets).
    SendError(String),
    /// Failure while protecting or unprotecting RTP packets.
    ProtectionError(String),
    /// A poisoned mutex or lock was encountered.
    PoisonedLock,
}

/// Formats a readable representation of the media pipeline error.
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
