use std::fmt::Display;

#[derive(Debug)]
/// Errors emitted by the server layer when handling connections and users.
pub enum ServerError {
    /// Failed to open the user data file.
    OpenUserDataFileError,
    /// Failed to write the user data file.
    WriteUserDataFileError,
    /// Attempted to operate on an unknown user.
    UserDoesNotExist(String),
    /// A synchronization primitive was poisoned.
    PoisonedLock,
    /// User is busy or otherwise unavailable.
    UserNotAvailable(String),
    /// Generic mapping/conversion error wrapper.
    MapError(String),
    /// Could not determine a valid server IP address.
    IPNotFound(String),
    /// Failed to bind a listening socket.
    FailedToBindAddress,
    /// Failed to establish a connection.
    ConnectionError(String),
    /// Server has been turned off.
    ServerOff,
    /// Input did not meet expected format.
    InvalidFormat,
}

impl Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenUserDataFileError => write!(f, "Error: failed to open user data file"),
            Self::WriteUserDataFileError => write!(f, "Error: failed to write user data file"),
            Self::UserDoesNotExist(usr_name) => write!(f, "Error: user does not exist: {usr_name}"),
            Self::PoisonedLock => write!(f, "Error: poisoned lock"),
            Self::UserNotAvailable(user_name) => {
                write!(f, "Error: user not available: {user_name}")
            }
            Self::MapError(e) => write!(f, "Error: {e}"),
            Self::IPNotFound(e) => write!(f, "Error: IP not found: {e}"),
            Self::FailedToBindAddress => write!(f, "Error: failed to bind address"),
            Self::ConnectionError(e) => write!(f, "Error: failed to connect: {e}"),
            Self::ServerOff => write!(f, "Error: is no longer available"),
            Self::InvalidFormat => write!(f, "Error: invalid format"),
        }
    }
}
