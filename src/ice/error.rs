use std::fmt::{Display, Formatter};

#[derive(PartialEq, Eq, Debug)]
pub enum IceError {
    NetworkInterfaceError,
    NoNetworkInterfaceFound,
    InvalidCandidateAddress,
    InvalidCandidatePort,
    InvalidCandidateType,
    NoLocalCandidates,
    NoRemoteCandidates,
    CandidatePairCreationFailed,
    NoCandidatePairs,
    InvalidConnectivityState,
    UnsupportedCandidateType,
    InternalError,
}

impl Display for IceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IceError::NetworkInterfaceError => write!(f, "Error: \"could not obtain network interface\""),
            IceError::NoNetworkInterfaceFound => write!(f, "Error: \"no network interface found\""),
            IceError::InvalidCandidateAddress => write!(f, "Error: \"invalid candidate address\""),
            IceError::InvalidCandidatePort => write!(f, "Error: \"invalid candidate port\""),
            IceError::InvalidCandidateType => write!(f, "Error: \"invalid candidate type\""),
            IceError::NoLocalCandidates => write!(f, "Error: \"no local candidate available\""),
            IceError::NoRemoteCandidates => write!(f, "Error: \"no remote candidate available\""),
            IceError::CandidatePairCreationFailed => write!(f, "Error: \"pair creation failed\""),
            IceError::NoCandidatePairs => write!(f, "Error: \"no candidate pairs available\""),
            IceError::InvalidConnectivityState => write!(f, "Error: \"invalid connectivity state\""),
            IceError::UnsupportedCandidateType => write!(f, "Error: \"unsupported candidate type\""),
            IceError::InternalError => write!(f, "Error: \"internal error\""),
        }
    }
}