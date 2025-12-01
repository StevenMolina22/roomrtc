use super::candidate::Candidate;
use super::candidate_pair::CandidatePair;
use super::connectivity_state::ConnectivityState;
use super::error::IceError as Error;
use crate::config::IceConfig;
use if_addrs;
use crate::session::ice::stun_client;
use crate::logger::Logger;

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
    logger: Logger,
}

impl IceAgent {
    /// Create a new, empty `IceAgent`.
    ///
    /// The agent starts with no local or remote candidates and no pairs.
    #[must_use]
    pub fn new(logger: Logger) -> Self {
        Self {
            local_candidates: Vec::new(),
            remote_candidates: Vec::new(),
            candidate_pairs: Vec::new(),
            selected_pair: None,
            logger,
        }
    }

    /// Gather local candidates and add them to the agent.
    ///
    /// This implementation performs a minimal gather: it selects a single
    /// non-loopback IPv4 address from the host and creates a host candidate.
    /// Additionally, it attempts STUN discovery using the provided `socket`.
    /// On successful STUN response a server-reflexive (srflx) candidate will
    /// be created and added to the agent's local candidates.
    ///
    /// # Arguments
    ///
    /// - `socket`: a bound UDP socket that will be used for STUN discovery and
    ///   from which the local host candidate's port is taken.
    /// - `ice_config`: configuration used to build candidates (priorities,
    ///   foundation, transport, component id, etc.).
    ///
    /// # Returns
    ///
    /// - `Ok(())` on success (local candidates added, STUN may succeed or fail
    ///   silently).
    /// - `Err(Error)` if a fatal error occurred (e.g., network interface
    ///   enumeration failure). STUN discovery failures are logged
    pub fn gather_candidates(&mut self, socket: &std::net::UdpSocket, ice_config: &IceConfig) -> Result<(), Error> {
    if let Ok(local_ip) = get_local_ip() {
        let port = socket.local_addr().map(|a| a.port()).unwrap_or(0);
        let mut host = Candidate::new_host(local_ip, ice_config.component_id, ice_config);
        host.port = port;
        self.local_candidates.push(host);
    }

    self.logger.info("STUN: Starting discovery on existing socket...");

    match stun_client::get_public_ip_and_port(socket) {
        Ok(addr) => {
            if let Some((ip, port_str)) = addr.split_once(':')
                && let Ok(stun_port) = port_str.parse::<u16>() {
                    self.logger.info(&format!("STUN OK: {}:{}", ip, stun_port));

                    let srflx = Candidate::new(
                        crate::session::ice::candidate_type::CandidateType::ServerReflexive,
                        ice_config.srflx_priority_preference * 1000,
                        ip.to_string(),
                        stun_port,
                        ice_config.component_id,
                        ice_config.foundation.clone(),
                        ice_config.transport.clone(),
                    );
                    self.local_candidates.push(srflx);
                }

        },
        Err(e) => self.logger.warn(&format!("STUN failed: {}", e)),
    }

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
    ///
    /// Note: server-reflexive (srflx) candidates discovered via STUN are
    /// skipped when forming Host-Host pairs in this implementation.

    fn create_candidate_pair(&mut self) -> Result<(), Error> {
        if self.local_candidates.is_empty() {
            return Err(Error::NoLocalCandidates);
        }

        if self.remote_candidates.is_empty() {
            return Err(Error::NoRemoteCandidates);
        }

        for local in &self.local_candidates {
            for remote in &self.remote_candidates {
                let is_local_stun = local.candidate_type == crate::session::ice::candidate_type::CandidateType::ServerReflexive;
                let is_remote_stun = remote.candidate_type == crate::session::ice::candidate_type::CandidateType::ServerReflexive;

                if is_local_stun || is_remote_stun {
                    self.logger.debug(&format!(
                        "Skipping STUN pair: {} <-> {}",
                        local.address, remote.address
                    ));
                    continue;
                }
                // -------------------------------------------------------------

                let pair = CandidatePair::new(local.clone(), remote.clone());
                self.candidate_pairs.push(pair);
            }
        }

        if self.candidate_pairs.is_empty() {
            self.logger.error("Error: No viable Host-Host pairs (are hosts on different networks?)");
            return Err(Error::NoCandidatePairs);
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
        self.logger.info("Handshake complete! A direct connection can be established.");
        self.logger.info(&format!(
            "   - My Address: {}:{}",
            selected_pair.local.address, selected_pair.local.port
        ));
        self.logger.info(&format!(
            "   - Peer Address: {}:{}",
            selected_pair.remote.address, selected_pair.remote.port
        ));

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

    pub fn get_local_candidates(&self) -> &[Candidate] {
        &self.local_candidates
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
    use crate::session::ice::candidate::Candidate;
    use crate::session::ice::candidate_type::CandidateType;
    use std::net::UdpSocket;

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
        let logger = Logger::new("test_ice_agent.log").unwrap();
        let agent = IceAgent::new(logger);

        assert!(agent.local_candidates.is_empty());
        assert!(agent.remote_candidates.is_empty());
        assert!(agent.candidate_pairs.is_empty());
        assert!(agent.get_selected_pair().is_err());
    }

    #[test]
    fn test_gather_candidates_adds_local_candidate() {
        let logger = Logger::new("test_ice_agent.log").unwrap();
        let mut agent = IceAgent::new(logger);
        let ice_config = IceConfig {
            foundation: "1".to_string(),
            transport: "UDP".to_string(),
            component_id: 1,
            host_priority_preference: 126,
            srflx_priority_preference: 100,
            host_local_preference: 65535,
        };

        let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind test socket");
        let assigned_port = socket.local_addr().unwrap().port();

        let result = agent.gather_candidates(&socket, &make_ice_config());

        if matches!(
            result,
            Err(Error::NetworkInterfaceError | Error::NoNetworkInterfaceFound)
        ) {
            eprintln!("Skipping gather_candidates test due to missing network interface");
            return;
        }
        assert!(result.is_ok());

        assert!(!agent.local_candidates.is_empty());

        let host_candidate = agent.local_candidates.iter()
            .find(|c| c.candidate_type == CandidateType::Host)
            .expect("Host candidate missing");

        assert_eq!(host_candidate.port, assigned_port);
    }

    #[test]
    fn test_add_remote_candidate_creates_pairs() -> Result<(), Error> {
        let logger = Logger::new("test_ice_agent.log").unwrap();
        let mut agent = IceAgent::new(logger);

        let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind test socket");
        agent.gather_candidates(&socket, &make_ice_config())?;

        let remote = sample_remote_candidate();

        let result = agent.add_remote_candidate(remote.clone());
        assert!(result.is_ok());

        assert_eq!(agent.remote_candidates.len(), 1);

        assert!(!agent.candidate_pairs.is_empty());

        let pair = &agent.candidate_pairs[0];
        assert_eq!(pair.local.address, agent.local_candidates[0].address);
        assert_eq!(pair.remote.address, remote.address);
        Ok(())
    }

    #[test]
    fn test_start_connectivity_checks_selects_pair() -> Result<(), Error> {
        let logger = Logger::new("test_ice_agent.log").unwrap();
        let mut agent = IceAgent::new(logger);

        let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind test socket");
        agent.gather_candidates(&socket, &make_ice_config())?;

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
        let logger = Logger::new("test_ice_agent.log").unwrap();
        let mut agent = IceAgent::new(logger);
        let result = agent.start_connectivity_checks();
        assert!(matches!(result, Err(Error::NoCandidatePairs)));
    }
}