use std::fmt::{Display, Formatter};

#[derive(PartialEq, Eq, Debug)]
pub enum IceErrors {
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

impl Display for IceErrors {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IceErrors::NetworkInterfaceError => write!(f, "Error: \"could not obtain network interface\""),
            IceErrors::NoNetworkInterfaceFound => write!(f, "Error: \"no network interface found\""),
            IceErrors::InvalidCandidateAddress => write!(f, "Error: \"invalid candidate address\""),
            IceErrors::InvalidCandidatePort => write!(f, "Error: \"invalid candidate port\""),
            IceErrors::InvalidCandidateType => write!(f, "Error: \"invalid candidate type\""),
            IceErrors::NoLocalCandidates => write!(f, "Error: \"no local candidate available\""),
            IceErrors::NoRemoteCandidates => write!(f, "Error: \"no remote candidate available\""),
            IceErrors::CandidatePairCreationFailed => write!(f, "Error: \"pair creation failed\""),
            IceErrors::NoCandidatePairs => write!(f, "Error: \"no candidate pairs available\""),
            IceErrors::InvalidConnectivityState => write!(f, "Error: \"invalid connectivity state\""),
            IceErrors::UnsupportedCandidateType => write!(f, "Error: \"unsupported candidate type\""),
            IceErrors::InternalError => write!(f, "Error: \"internal error\""),
        }
    }
}