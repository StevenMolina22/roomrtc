use std::fmt::{Display, Formatter};

/// Error type used by the ICE module.
///
/// Encapsulates the possible failure modes that the ICE
/// implementation may encounter while gathering interfaces, creating
/// candidate pairs, or performing connectivity selection.
#[derive(PartialEq, Eq, Debug)]
pub enum IceError {
    /// Underlying OS call to enumerate network interfaces failed.
    NetworkInterfaceError,

    /// No suitable non-loopback network interface was found on the host.
    NoNetworkInterfaceFound,

    /// Provided candidate address is invalid or unparsable.
    InvalidCandidateAddress,

    /// Provided candidate port is invalid.
    InvalidCandidatePort,

    /// The candidate type string could not be parsed or is invalid.
    InvalidCandidateType,

    /// No local candidates were gathered before attempting pair creation.
    NoLocalCandidates,

    /// No remote candidates were provided before attempting pair creation.
    NoRemoteCandidates,

    /// Failed to create candidate pairs for unknown reasons.
    CandidatePairCreationFailed,

    /// No candidate pairs are available when attempting connectivity checks.
    NoCandidatePairs,

    /// An invalid connectivity state was observed/used.
    InvalidConnectivityState,

    /// The provided candidate type is syntactically supported but not
    /// implemented by this agent.
    UnsupportedCandidateType,

    /// Internal or unexpected error.
    InternalError,

    /// No candidate pair has been selected yet.
    NoSelectedPair,
}

impl Display for IceError {
    /// Format the error as a short human-readable message.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NetworkInterfaceError => {
                write!(f, "Error: \"could not obtain network interface\"")
            }
            IceError::NoNetworkInterfaceFound => write!(f, "Error: \"no network interface found\""),
            IceError::InvalidCandidateAddress => write!(f, "Error: \"invalid candidate address\""),
            IceError::InvalidCandidatePort => write!(f, "Error: \"invalid candidate port\""),
            IceError::InvalidCandidateType => write!(f, "Error: \"invalid candidate type\""),
            IceError::NoLocalCandidates => write!(f, "Error: \"no local candidate available\""),
            IceError::NoRemoteCandidates => write!(f, "Error: \"no remote candidate available\""),
            IceError::CandidatePairCreationFailed => write!(f, "Error: \"pair creation failed\""),
            IceError::NoCandidatePairs => write!(f, "Error: \"no candidate pairs available\""),
            IceError::InvalidConnectivityState => {
                write!(f, "Error: \"invalid connectivity state\"")
            }
            Self::UnsupportedCandidateType => {
                write!(f, "Error: \"unsupported candidate type\"")
            }
            IceError::InternalError => write!(f, "Error: \"internal error\""),
            IceError::NoSelectedPair => write!(f, "Error: \"no selected pair available\""),
        }
    }
}
