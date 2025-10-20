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
