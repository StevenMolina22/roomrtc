/// Connectivity state for a candidate pair or connectivity checks.
///
/// This enum represents the current state of connectivity probing for an
/// ICE candidate pair. It is used to track progress of checks and to
/// communicate status in logs and user interfaces.
#[derive(Debug, PartialEq, Clone)]
pub enum ConnectivityState {
    /// Initial state: the pair is waiting to be checked.
    Waiting,

    /// Checks are currently in progress for the pair.
    InProgress,

    /// At least one check succeeded and the pair is considered connected.
    Succeeded,

    /// All checks failed and the pair is considered unusable.
    Failed,
}

impl std::fmt::Display for ConnectivityState {
    /// Format the connectivity state as a human-readable string.
    ///
    /// Produces names for logs and UIs.
    /// `"Waiting"`, `"In Progress"`, `"Succeeded"`, `"Failed"`.
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
