use std::fmt::Display;

#[derive(Debug)]
pub enum ServerError {
    OpenUserDataFileError,
    WriteUserDataFileError,
    UserDoesNotExist(String),
    PoisonedLock,
    UserNotAvailable(String),
    MapError(String),
    IPNotFound(String),
    FailedToBindAddress,
    ConnectionError(String),
    ServerOff,
    InvalidFormat
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
