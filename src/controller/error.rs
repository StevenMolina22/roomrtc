use std::fmt::Display;
use std::net::AddrParseError;

/// Errors that can occur when operating the controller.
///
/// This enum collects the various failure modes the controller may
/// expose when creating sockets, interacting with RTP senders and
/// receivers, managing threads, or working with internal maps and
/// locks. Variants carry contextual error information where
/// available.

#[derive(Debug, Eq, PartialEq)]
pub enum ControllerError {
    Error(String),
    ConnectingToServerFailed,
    IOError(String),
    BadResponse,
    LogInFailed(String),
    SignUpFailed(String),
    LogOutFailed(String),

    /// Failed to parse a socket address.
    ParsingSocketAddressError(AddrParseError),

    /// Failed to bind a socket to the requested address.
    BindingAddressError(String),

    /// General socket connection error.
    ConnectionSocketError(String),

    /// Error cloning a UDP socket for use across threads.
    CloningSocketError(String),

    /// A poisoned mutex or lock was encountered.
    PoisonedLock,

    /// Generic mapping/lookup error carrying a message.
    MapError(String),

    /// Connection has not been started when an operation required it.
    ConnectionNotStarted,

    /// The connection was closed while an operation was in progress.
    ConnectionClosed,

    NotLoggedInError,
    CallError(String),
    CallRefused,
}

impl Display for ControllerError {
    /// Format a readable representation of the controller error.
    ///
    /// These messages include brief context and the underlying error details where present.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::Error(e) => write!(f, "{e}"),
            Self::ConnectingToServerFailed => write!(f, "Error: \"Connecting to server failed\""),
            Self::IOError(e) => write!(f, "Error: \"IO Error\". Details: {e}"),
            Self::BadResponse => write!(f, "Error: \"Unexpected response from the server\""),
            Self::LogInFailed(e) => write!(f, "Error: \"Failed to log in: {e}\""),
            Self::SignUpFailed(e) => write!(f, "Error: \"Failed to sign up: {e}\""),
            Self::LogOutFailed(e) => write!(f, "Error: \"Failed to log out: {e}\""),

            Self::ParsingSocketAddressError(e) => {
                write!(f, "Error: \"Failed to parse socket address\". Details: {e}")
            }
            Self::BindingAddressError(e) => {
                write!(f, "Error: \"Failed to bind socket\". Details: {e}")
            }
            Self::ConnectionSocketError(e) => {
                write!(f, "Error: \"Failed to connect\". Details: {e}")
            }
            Self::CloningSocketError(e) => {
                write!(f, "Error: \"Failed to clone UDP socket\" Details: {e}")
            }
            Self::PoisonedLock => write!(f, "Error: \"Poisoned lock\""),
            Self::MapError(e) => write!(f, "{e}"),
            Self::ConnectionNotStarted => write!(f, "Error: \"Connection not started\""),
            Self::ConnectionClosed => write!(f, "Error: \"Connection closed\""),
            Self::NotLoggedInError => write!(f, "Error: \"Not logged in\""),
            Self::CallError(e) => write!(f, "Error: \"Start call failed\": {e}"),
            Self::CallRefused => write!(f, "Error: \"Call refused by peer\""),
        }
    }
}
