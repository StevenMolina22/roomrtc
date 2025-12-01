use std::fmt::Display;

pub enum CameraError {
    PoisonedLock,
}

impl Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::PoisonedLock => write!(f, "Error: \"Poisoned lock\""),
        }
    }
}
