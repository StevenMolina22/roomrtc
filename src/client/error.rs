use std::fmt::Display;

/// Errors returned by the client-facing signaling helpers.
///
/// This small enum wraps errors coming from SDP handling and ICE
/// connectivity operations. Each variant carries
/// string describing the underlying problem.
#[derive(Eq, PartialEq, Debug)]
pub enum ClientError {
    /// Error produced while parsing or creating an SDP message.
    SdpCreationError(String),

    /// Error produced while performing ICE-related operations
    /// (adding remote candidates, starting connectivity checks, etc.).
    IceConnectionError(String),
}

/// Provide a compact representation for `ClientError`.
///
/// The implementation forwards the contained string so callers that
/// format the error (for logs or UI) receive the original message.
impl Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::SdpCreationError(e) => write!(f, "{e}"),
            Self::IceConnectionError(e) => write!(f, "{e}"),
        }
    }
}
