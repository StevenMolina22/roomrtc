use super::candidate::Candidate;
use super::candidate_pair::CandidatePair;
use super::connectivity_state::ConnectivityState;
use super::error::IceError as Error;

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
    pub const fn get_selected_pair(&self) -> Option<&CandidatePair> {
        self.selected_pair.as_ref()
    }
}

/// Gets the local IP using if_addrs
fn get_local_ip() -> Result<String, Error> {
    // Get all network interfaces
    // returns a list with Interface type objects
    // each Interface has name, addr, index, oper_status and is_loopback() method
    let interfaces =
        if_addrs::get_if_addrs().map_err(|_| Error::NetworkInterfaceError)?;

    // Find the first interface that is not loopback
    // loopback is a virtual internal interface of the operating system (doesn't connect any local network)
    for interface in interfaces {
        if !interface.is_loopback() {
            return Ok(interface.addr.ip().to_string());
        }
    }

    Err(Error::NoNetworkInterfaceFound)
}
