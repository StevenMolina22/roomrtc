#[derive(Debug, PartialEq, Clone)]
pub enum ConnectivityState {
    Waiting,
    InProgress,
    Succeeded,
    Failed,
}

impl std::fmt::Display for ConnectivityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Waiting => write!(f, "Waiting"),
            Self::InProgress => write!(f, "In Progress"),
            Self::Succeeded => write!(f, "Succeeded"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ConnectivityState;

    #[test]
    fn test_connectivity_state_display() {
        assert_eq!(ConnectivityState::Waiting.to_string(), "Waiting");
        assert_eq!(ConnectivityState::InProgress.to_string(), "In Progress");
        assert_eq!(ConnectivityState::Succeeded.to_string(), "Succeeded");
        assert_eq!(ConnectivityState::Failed.to_string(), "Failed");
    }
}
