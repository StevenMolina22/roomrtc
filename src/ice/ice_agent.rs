use super::candidate::Candidate;
use super::candidate_pair::CandidatePair;
use super::connectivity_state::ConnectivityState;
use super::error::IceError as Error;
use if_addrs;

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
    /// Creates a new empty ICE agent
    #[must_use]
    pub const fn new() -> Self {
        Self {
            local_candidates: Vec::new(),
            remote_candidates: Vec::new(),
            candidate_pairs: Vec::new(),
            selected_pair: None,
        }
    }

    /// Gathers local candidates (available IP addresses)
    /// This implementation only gets the local IP
    pub fn gather_candidates(&mut self, port: u16) -> Result<(), Error> {
        // Find my local IP and create a candidate
        let local_ip = get_local_ip()?;
        let mut candidate = Candidate::new_host(local_ip, 1);
        candidate.port = port;
        self.local_candidates.push(candidate);

        // In a real ICE implementation, here would be STUN/TURN interaction (final delivery)
        Ok(())
    }

    /// Adds a single remote candidate and creates pairs
    pub fn add_remote_candidate(&mut self, candidate: Candidate) -> Result<(), Error> {
        self.remote_candidates.push(candidate);
        self.create_candidate_pair()
    }

    /// Adds remote candidates received from the other peer and creates candidate pairs
    pub fn add_remote_candidates(&mut self, candidates: Vec<Candidate>) -> Result<(), Error> {
        for candidate in candidates {
            self.remote_candidates.push(candidate);
        }
        self.create_candidate_pair()
    }

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

    /// Starts connectivity checks and selects the best pair
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

    #[must_use]
    pub fn get_local_candidate(&self) -> Option<&Candidate> {
        self.local_candidates.first()
    }

    #[must_use]
    pub fn get_selected_pair(&self) -> Result<&CandidatePair, Error> {
        self.selected_pair.as_ref().ok_or(Error::NoSelectedPair)
    }
}

/// Gets the local IP using if_addrs
fn get_local_ip() -> Result<String, Error> {
    let interfaces = if_addrs::get_if_addrs().map_err(|_| Error::NetworkInterfaceError)?;

    for interface in interfaces {
        if !interface.is_loopback() {
            if let std::net::IpAddr::V4(ipv4) = interface.addr.ip() {
                return Ok(ipv4.to_string());
            }
        }
    }

    Err(Error::NoNetworkInterfaceFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ice::candidate::Candidate;
    use crate::ice::candidate_type::CandidateType;

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

        let result = agent.gather_candidates(3478);
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
        agent.gather_candidates(4000)?;

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
        agent.gather_candidates(4000)?;

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
