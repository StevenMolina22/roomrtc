use super::error::IceError as Error;
use std::str::FromStr;

#[derive(Debug, PartialEq, Clone)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_type_priority() {
        assert_eq!(CandidateType::Host.priority(), 126);
        assert_eq!(CandidateType::ServerReflexive.priority(), 100);
    }

    #[test]
    fn test_candidate_type_display() {
        assert_eq!(CandidateType::Host.to_string(), "host");
        assert_eq!(CandidateType::ServerReflexive.to_string(), "srflx");
    }

    #[test]
    fn test_candidate_type_from_str() {
        assert_eq!(
            CandidateType::from_str("host").unwrap(),
            CandidateType::Host
        );
        assert_eq!(
            CandidateType::from_str("srflx").unwrap(),
            CandidateType::ServerReflexive
        );
        assert!(CandidateType::from_str("invalid").is_err());
    }
}
