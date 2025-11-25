use super::candidate::Candidate;
use super::candidate_pair::CandidatePair;
use super::connectivity_state::ConnectivityState;
use super::error::IceError as Error;
use crate::config::IceConfig;
use if_addrs;

/// An ICE agent that gathers local candidates, accepts remote candidates
/// and forms candidate pairs for connectivity checks.
///
/// This agent implements a small subset of ICE functionality:
/// gathering local host candidates, adding
/// remote candidates, creating candidate pairs, and selecting a pair after
/// simulated connectivity checks.
pub struct IceAgent {
    local_candidates: Vec<Candidate>,
    remote_candidates: Vec<Candidate>,
    candidate_pairs: Vec<CandidatePair>,
    selected_pair: Option<CandidatePair>,
}

impl Default for IceAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl IceAgent {
    /// Create a new, empty `IceAgent`.
    ///
    /// The agent starts with no local or remote candidates and no pairs.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            local_candidates: Vec::new(),
            remote_candidates: Vec::new(),
            candidate_pairs: Vec::new(),
            selected_pair: None,
        }
    }

    /// Gather local candidates and add them to the agent.
    ///
    /// This implementation performs a minimal gather: it selects a single
    /// non-loopback IPv4 address from the host and creates a host
    /// candidate.
    pub fn gather_candidates(&mut self, port: u16, ice_config: &IceConfig) -> Result<(), Error> {
        let local_ip = get_local_ip()?;
        let mut candidate = Candidate::new_host(local_ip, ice_config.component_id, ice_config);
        candidate.port = port;
        self.local_candidates.push(candidate);

        // In a real ICE implementation, here would be STUN/TURN interaction (final delivery)
        Ok(())
    }

    /// Add a single remote candidate and create candidate pairs.
    ///
    /// Returns an error if no local candidates are available.
    pub fn add_remote_candidate(&mut self, candidate: Candidate) -> Result<(), Error> {
        self.remote_candidates.push(candidate);
        self.create_candidate_pair()
    }

    /// Add multiple remote candidates (e.g., received from the peer) and
    /// create candidate pairs for them.
    pub fn add_remote_candidates(&mut self, candidates: Vec<Candidate>) -> Result<(), Error> {
        for candidate in candidates {
            self.remote_candidates.push(candidate);
        }
        self.create_candidate_pair()
    }

    /// Create pairs between all local and remote candidates.
    ///
    /// Validates that both local and remote candidate lists are non-empty
    /// and constructs `CandidatePair` instances for every combination.
    fn create_candidate_pair(&mut self) -> Result<(), Error> {
        if self.local_candidates.is_empty() {
            return Err(Error::NoLocalCandidates);
        }

        if self.remote_candidates.is_empty() {
            return Err(Error::NoRemoteCandidates);
        }

        for local in &self.local_candidates {
            for remote in &self.remote_candidates {
                let pair = CandidatePair::new(local.clone(), remote.clone());
                self.candidate_pairs.push(pair);
            }
        }
        Ok(())
    }

    /// Start connectivity checks and select a working pair.
    ///
    /// This function simulates connectivity checks by selecting the first
    /// pair and marking it as `Succeeded`.
    pub fn start_connectivity_checks(&mut self) -> Result<(), Error> {
        if self.candidate_pairs.is_empty() {
            return Err(Error::NoCandidatePairs);
        }

        // The first pair is selected (highest priority)
        // In a real implementation, here would be STUN verifications (final delivery)
        let mut selected_pair = self.candidate_pairs[0].clone();
        selected_pair.state = ConnectivityState::Succeeded;

        self.selected_pair = Some(selected_pair.clone());

        // Display handshake completion message
        eprintln!("Handshake complete! A direct connection can be established.");
        eprintln!(
            "   - My Address: {}:{}",
            selected_pair.local.address, selected_pair.local.port
        );
        eprintln!(
            "   - Peer Address: {}:{}",
            selected_pair.remote.address, selected_pair.remote.port
        );

        Ok(())
    }

    pub fn get_local_ip_str(&self) -> Result<String, Error> {
        get_local_ip()
    }

    /// Return a reference to the first local candidate, if any.
    #[must_use]
    pub fn get_local_candidate(&self) -> Option<&Candidate> {
        self.local_candidates.first()
    }

    /// Return the selected candidate pair or an error if none was selected.
    pub fn get_selected_pair(&self) -> Result<&CandidatePair, Error> {
        self.selected_pair.as_ref().ok_or(Error::NoSelectedPair)
    }

    /// Clean remote candidates and candidate pairs.
    pub fn clean_remote_candidates(&mut self) {
        self.remote_candidates.clear();
        self.candidate_pairs.clear();
        self.selected_pair = None;
    }
}

/// Get a non-loopback IPv4 address from the host using `if_addrs`.
///
/// Returns an error if no suitable interface is found or if the
/// underlying call to list interfaces fails.
fn get_local_ip() -> Result<String, Error> {
    let interfaces = if_addrs::get_if_addrs().map_err(|_| Error::NetworkInterfaceError)?;

    for interface in interfaces {
        if !interface.is_loopback()
            && let std::net::IpAddr::V4(ipv4) = interface.addr.ip()
        {
            return Ok(ipv4.to_string());
        }
    }

    Err(Error::NoNetworkInterfaceFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ice::candidate::Candidate;
    use crate::ice::candidate_type::CandidateType;

    fn make_ice_config() -> IceConfig {
        IceConfig {
            foundation: "1".to_string(),
            transport: "UDP".to_string(),
            component_id: 1,
            host_priority_preference: 126,
            srflx_priority_preference: 100,
            host_local_preference: 65535,
        }
    }

    fn sample_remote_candidate() -> Candidate {
        Candidate::new(
            CandidateType::Host,
            126,
            "192.168.0.50".to_string(),
            5000,
            1,
            "1".to_string(),
            "udp".to_string(),
        )
    }

    #[test]
    fn test_new_agent_is_empty() {
        let agent = IceAgent::new();

        assert!(agent.local_candidates.is_empty());
        assert!(agent.remote_candidates.is_empty());
        assert!(agent.candidate_pairs.is_empty());
        assert!(agent.get_selected_pair().is_err());
    }

    #[test]
    fn test_gather_candidates_adds_local_candidate() {
        let mut agent = IceAgent::new();
        let ice_config = IceConfig {
            foundation: "1".to_string(),
            transport: "UDP".to_string(),
            component_id: 1,
            host_priority_preference: 126,
            srflx_priority_preference: 100,
            host_local_preference: 65535,
        };

        let result = agent.gather_candidates(3478, &make_ice_config());
        if matches!(
            result,
            Err(Error::NetworkInterfaceError | Error::NoNetworkInterfaceFound)
        ) {
            eprintln!("Skipping gather_candidates test due to missing network interface");
            return;
        }
        assert!(result.is_ok());

        assert_eq!(agent.local_candidates.len(), 1);
        assert_eq!(agent.local_candidates[0].port, 3478);
        assert_eq!(
            agent.local_candidates[0].candidate_type,
            CandidateType::Host
        );
    }

    #[test]
    fn test_add_remote_candidate_creates_pairs() -> Result<(), Error> {
        let mut agent = IceAgent::new();
        agent.gather_candidates(4000, &make_ice_config())?;

        let remote = sample_remote_candidate();

        let result = agent.add_remote_candidate(remote.clone());
        assert!(result.is_ok());

        assert_eq!(agent.remote_candidates.len(), 1);
        assert_eq!(agent.candidate_pairs.len(), 1);

        let pair = &agent.candidate_pairs[0];
        assert_eq!(pair.local.address, agent.local_candidates[0].address);
        assert_eq!(pair.remote.address, remote.address);
        Ok(())
    }

    #[test]
    fn test_start_connectivity_checks_selects_pair() -> Result<(), Error> {
        let mut agent = IceAgent::new();
        agent.gather_candidates(5000, &make_ice_config())?;

        let remote = sample_remote_candidate();
        agent.add_remote_candidate(remote)?;

        let result = agent.start_connectivity_checks();
        assert!(result.is_ok());

        let selected = agent.get_selected_pair()?;
        assert_eq!(selected.state, ConnectivityState::Succeeded);

        Ok(())
    }

    #[test]
    fn test_error_when_starting_checks_without_pairs() {
        let mut agent = IceAgent::new();
        let result = agent.start_connectivity_checks();
        assert!(matches!(result, Err(Error::NoCandidatePairs)));
    }
}
