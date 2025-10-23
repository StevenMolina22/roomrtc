use super::error::IceError as Error;
use std::str::FromStr;
#[derive(Clone)]
pub enum CandidateType {
    Host,
    ServerReflexive,
}

impl CandidateType {
    /// Returns the candidate type priority according to RFC 8445
    #[must_use]
    pub const fn priority(&self) -> u32 {
        match self {
            Self::Host => 126,
            Self::ServerReflexive => 100,
        }
    }
}

impl std::fmt::Display for CandidateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host => write!(f, "host"),
            Self::ServerReflexive => write!(f, "srflx"),
        }
    }
}

impl FromStr for CandidateType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "host" => Ok(Self::Host),
            "srflx" => Ok(Self::ServerReflexive),
            _ => Err(Error::InvalidCandidateType),
        }
    }
}
