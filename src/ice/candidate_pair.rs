use crate::ice::candidate::Candidate;
use crate::ice::connectivity_state::ConnectivityState;

#[derive(Clone)]
pub struct CandidatePair {
    pub local: Candidate,
    pub remote: Candidate,
    pub priority: u64,
    pub state: ConnectivityState,
}

impl CandidatePair {
    #[must_use]
    pub fn new(local: Candidate, remote: Candidate) -> Self {
        let priority = Self::calculate_pair_priority(&local, &remote);
        Self {
            local,
            remote,
            priority,
            state: ConnectivityState::Waiting,
        }
    }

    fn calculate_pair_priority(local: &Candidate, remote: &Candidate) -> u64 {
        let g = u64::from(std::cmp::min(local.priority, remote.priority));
        let l = u64::from(std::cmp::max(local.priority, remote.priority));
        (1u64 << 32) * g + 2 * l + u64::from(local.priority > remote.priority)
    }
}

impl std::fmt::Display for CandidatePair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} <-> {} [{}] (priority: {})",
            self.local, self.remote, self.state, self.priority
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ice::candidate_type::CandidateType;
    use crate::ice::connectivity_state::ConnectivityState;

    fn build_candidate(priority: u32) -> Candidate {
        Candidate {
            candidate_type: CandidateType::Host,
            priority,
            address: "127.0.0.1".to_string(),
            port: 5000,
            component_id: 1,
            foundation: "foundation".to_string(),
            transport: "udp".to_string(),
        }
    }

    #[test]
    fn test_candidate_pair_new_initialization() {
        let local = build_candidate(100);
        let remote = build_candidate(200);

        let pair = CandidatePair::new(local.clone(), remote.clone());

        assert_eq!(pair.local.priority, 100);
        assert_eq!(pair.remote.priority, 200);
        assert_eq!(pair.state, ConnectivityState::Waiting);
    }

    #[test]
    fn test_candidate_pair_priority_calculation() {
        // local = 300, remote = 100
        let local = build_candidate(300);
        let remote = build_candidate(100);

        let pair = CandidatePair::new(local, remote);

        // Fórmula:
        // g = min(300, 100) = 100
        // l = max(300, 100) = 300
        // priority = (1<<32)*g + 2*l + (local > remote ? 1 : 0)
        let expected = ((1u64 << 32) * 100) + (2 * 300) + 1;

        assert_eq!(pair.priority, expected);
    }

    #[test]
    fn test_candidate_pair_display() {
        let local = build_candidate(123);
        let remote = build_candidate(456);

        let pair = CandidatePair::new(local, remote);

        let display = format!("{}", pair);

        assert!(display.contains("<->"));
        assert!(display.contains("priority"));
        assert!(display.contains("Waiting"));
        assert!(display.contains("127.0.0.1:5000"));
    }
}
