use std::fmt::Display;

pub enum CameraError {
    PoisonedLock,
    IndexError,
    OpenError(String),
    ClosedCamera,
    CameraConfigFailed,
}

impl Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::PoisonedLock => write!(f, "Error: \"Poisoned lock\""),
            Self::IndexError => write!(f, "Error: \"Device index is too large\""),
            Self::OpenError(e) => write!(f, "Error: \"Failed to open camera\": {e}"),
            Self::ClosedCamera => write!(f, "Error: \"Camera is closed\""),
            Self::CameraConfigFailed => write!(f, "Error: \"Failed to set camera configuration\""),
        }
    }
}
