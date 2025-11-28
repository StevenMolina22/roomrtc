use std::fmt::Display;

pub enum GUIError {
    MapError(String),
    EmptyReceiver,
}


impl Display for GUIError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::MapError(e) => write!(f, "{}", e),
            Self::EmptyReceiver => write!(f, "Receiver not started"),
        }
    }
}