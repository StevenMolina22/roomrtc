use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum DtlsError {
    InitializationSocketError,
    MapError(String),
}

impl Display for DtlsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InitializationSocketError => write!(f, "Error: failed to initialize DTLS socket"),
            Self::MapError(e) => write!(f, "Error: {e}"),
        }
    }
}
