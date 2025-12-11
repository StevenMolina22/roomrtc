use std::fmt::Display;

/// Errors returned by the session-facing signaling helpers.
///
/// This small enum wraps errors coming from SDP handling and ICE
/// connectivity operations. Each variant carries
/// string describing the underlying problem.
#[derive(Eq, PartialEq, Debug)]
pub enum CallSessionError {
    /// Error produced while parsing or creating an SDP message.
    SdpCreationError(String),

    /// Error produced while performing ICE-related operations
    /// (adding remote candidates, starting connectivity checks, etc.).
    IceConnectionError(String),
    BadAddress,

    /// Error produced while initializing the security context (certificates,
    /// DTLS identity, etc.).
    SecurityInitializationError(String),
}

/// Provide a compact representation for `CallSessionError`.
///
/// The implementation forwards the contained string so callers that
/// format the error (for logs or UI) receive the original message.
impl Display for CallSessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::BadAddress => write!(f, "Error: \"Bad address\""),

            Self::SdpCreationError(e)
            | Self::IceConnectionError(e)
            | Self::SecurityInitializationError(e) => {
                write!(f, "{e}")
            }
        }
    }
}
