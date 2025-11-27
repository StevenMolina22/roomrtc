use std::fmt::Display;

pub enum MediaPipelineError {
    MapError(String),
    ParsingError(String),
    SendError(String),
}

impl Display for MediaPipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::MapError(e) => write!(f, "{}", e),
        }
    }
}