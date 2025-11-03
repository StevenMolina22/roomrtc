use super::error::IceError as Error;
use std::str::FromStr;

/// Type of an ICE candidate.
///
/// Represents the origin category of an ICE candidate. The enum covers
/// the candidate types used in this implementation and provides helper
/// utilities such as a RFC-compliant priority value.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum CandidateType {
    /// Host candidate (direct local interface address).
    Host,

    /// Server reflexive candidate.
    ServerReflexive,
}

impl CandidateType {
    /// Returns the candidate type preference according to RFC 8445.
    ///
    /// The returned value is the type preference used when computing a
    /// candidate's overall priority. It is a small integer (u32) that
    /// reflects how desirable the candidate type is (higher is better).

    #[must_use]
    pub const fn priority(&self) -> u32 {
        match self {
            Self::Host => 126,
            Self::ServerReflexive => 100,
        }
    }
}

impl std::fmt::Display for CandidateType {
    /// Formats the candidate type as the short string used in SDP/ICE
    /// exchanges: `"host"` or `"srflx"`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host => write!(f, "host"),
            Self::ServerReflexive => write!(f, "srflx"),
        }
    }
}

impl FromStr for CandidateType {
    /// Parses a string into a `CandidateType`.
    ///
    /// Accepts the canonical short forms used in SDP/ICE: `"host"` and
    /// `"srflx"`. Returns `IceError::InvalidCandidateType` for unknown
    /// inputs.
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
    fn test_candidate_type_from_str() -> Result<(), Error> {
        assert_eq!(CandidateType::from_str("host")?, CandidateType::Host);
        assert_eq!(
            CandidateType::from_str("srflx")?,
            CandidateType::ServerReflexive
        );
        assert!(CandidateType::from_str("invalid").is_err());
        Ok(())
    }
}
