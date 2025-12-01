use std::fmt::Display;

pub enum GUIError {
    MapError(String),
    EmptyReceiver,
    ControllerDisconnected,
}

impl Display for GUIError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::MapError(e) => write!(f, "Error: \"{e}\""),
            Self::EmptyReceiver => write!(f, "Error: \"Receiver not started\""),
            Self::ControllerDisconnected => write!(f, "Error: \"Controller disconnected\""),
        }
    }
}
