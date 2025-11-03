use crate::ice::candidate::Candidate;
use crate::ice::connectivity_state::ConnectivityState;

/// A pair of ICE candidates consisting of a local and a remote candidate.
///
/// This `struct` represents a tuple (local, remote) used during the
/// connectivity-checking phase to probe candidate pairs. Each pair has a
/// priority computed according to the ICE formula (see
/// `calculate_pair_priority`) and a connectivity state.
#[derive(Clone)]
pub struct CandidatePair {
    /// The local candidate (from this agent).
    pub local: Candidate,

    /// The remote candidate (from the remote peer).
    pub remote: Candidate,

    /// Pair priority computed using the ICE formula.
    ///
    /// The priority is calculated from the `priority` fields of the local
    /// and remote candidates and stored as a `u64`.
    pub priority: u64,

    /// Current connectivity state of the pair.
    pub state: ConnectivityState,
}

impl CandidatePair {
    /// Creates a new `CandidatePair` from a local and remote candidate.
    ///
    /// # Parameters
    ///
    /// - `local`: the local `Candidate`.
    /// - `remote`: the remote `Candidate`.
    ///
    /// # Returns
    ///
    /// Returns a `CandidatePair` with the computed priority and the initial
    /// state set to `ConnectivityState::Waiting`.
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

    /// Computes the pair priority according to the ICE formula.
    ///
    /// The formula used is:
    ///
    /// g = min(local.priority, remote.priority)
    /// l = max(local.priority, remote.priority)
    /// priority = (1<<32) * g + 2*l + (local > remote ? 1 : 0)
    ///
    /// # Notes
    ///
    /// - This method is private because priority is computed internally when
    ///   constructing the pair.
    /// - Returns `u64` to avoid overflow when combining values.
    fn calculate_pair_priority(local: &Candidate, remote: &Candidate) -> u64 {
        let g = u64::from(std::cmp::min(local.priority, remote.priority));
        let l = u64::from(std::cmp::max(local.priority, remote.priority));
        (1u64 << 32) * g + 2 * l + u64::from(local.priority > remote.priority)
    }
}

impl std::fmt::Display for CandidatePair {
    /// Formats a `CandidatePair` as `"<local> <-> <remote> [<state>] (priority: <priority>)"`.
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

        let pair = CandidatePair::new(local, remote);

        assert_eq!(pair.local.priority, 100);
        assert_eq!(pair.remote.priority, 200);
        assert_eq!(pair.state, ConnectivityState::Waiting);
    }

    #[test]
    fn test_candidate_pair_priority_calculation() {
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

        let display = format!("{pair}");

        assert!(display.contains("<->"));
        assert!(display.contains("priority"));
        assert!(display.contains("Waiting"));
        assert!(display.contains("127.0.0.1:5000"));
    }
}
