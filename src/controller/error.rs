use std::fmt::Display;
use std::net::AddrParseError;

/// Errors that can occur when operating the controller.
///
/// This enum collects the various failure modes the controller may
/// expose when creating sockets, interacting with RTP media components,
/// managing threads, authentication operations, call handling, or working
/// with internal locks and maps. Variants carry contextual error information
/// where available.
///
/// # Variants
///
/// - `Error`: Generic error with a message.
/// - `ConnectingToServerFailed`: Failed to establish connection to server.
/// - `IOError`: Input/output operation failed.
/// - `BadResponse`: Unexpected response format from server.
/// - `LogInFailed`: Authentication login failed.
/// - `SignUpFailed`: User registration failed.
/// - `LogOutFailed`: Logout operation failed.
/// - `ParsingSocketAddressError`: Failed to parse socket address.
/// - `BindingAddressError`: Failed to bind socket to address.
/// - `ConnectionSocketError`: Socket connection establishment failed.
/// - `CloningSocketError`: Failed to clone socket for multi-threaded use.
/// - `PoisonedLock`: Poisoned mutex or RwLock encountered.
/// - `MapError`: Generic mapping/lookup operation error.
/// - `ConnectionNotStarted`: Connection not initialized when required.
/// - `ConnectionClosed`: Connection closed unexpectedly.
/// - `NotLoggedInError`: User not authenticated.
/// - `CallError`: Call operation failed.
/// - `CallRefused`: Call was rejected by peer.

#[derive(Debug, Eq, PartialEq)]
pub enum ControllerError {
    /// Generic error with a message.
    Error(String),

    /// Failed to establish connection to the server.
    ConnectingToServerFailed,

    /// Input/output operation error.
    IOError(String),

    /// Unexpected or malformed response from server.
    BadResponse,

    /// Login authentication failed.
    LogInFailed(String),

    /// User registration/signup failed.
    SignUpFailed(String),

    /// Logout operation failed.
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

    /// User is not logged in.
    NotLoggedInError,

    /// Call operation failed with details.
    CallError(String),

    /// Call was rejected by the peer.
    CallRefused,
}

impl Display for ControllerError {
    /// Formats a readable representation of the controller error.
    ///
    /// This implementation provides human-readable error messages that include
    /// context and underlying error details where available.
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
