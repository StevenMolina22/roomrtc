use super::candidate::Candidate;
use super::candidate_pair::CandidatePair;
use super::connectivity_state::ConnectivityState;
use super::error::IceError as Error;
use crate::config::IceConfig;
use crate::logger::Logger;
use crate::session::ice::candidate_type::CandidateType;
use crate::session::ice::stun_client;
use if_addrs;
use std::time::{Duration, Instant};

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
    pub const fn new(logger: Logger) -> Self {
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
    pub fn gather_candidates(
        &mut self,
        socket: &std::net::UdpSocket,
        ice_config: &IceConfig,
    ) -> Result<(), Error> {
        if let Ok(local_ip) = get_local_ip() {
            let port = match socket.local_addr() {
                Ok(addr) => addr.port(),
                Err(_) => 0,
            };
            let mut host = Candidate::new_host(local_ip, ice_config.component_id, ice_config);
            host.port = port;
            self.local_candidates.push(host);
        }

        self.logger
            .info("STUN: Starting discovery on existing socket...");

        match stun_client::get_public_ip_and_port(socket, &self.logger) {
            Ok(addr) => {
                if let Some((ip, port_str)) = addr.split_once(':')
                    && let Ok(stun_port) = port_str.parse::<u16>()
                {
                    self.logger.info(&format!("STUN OK: {ip}:{stun_port}"));

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
            }
            Err(e) => self.logger.warn(&format!("STUN failed: {e}")),
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
                let pair = CandidatePair::new(local.clone(), remote.clone());
                self.candidate_pairs.push(pair);
            }
        }

        if self.candidate_pairs.is_empty() {
            self.logger
                .error("Error: No viable Host-Host pairs (are hosts on different networks?)");
            return Err(Error::NoCandidatePairs);
        }

        Ok(())
    }

    /// Start connectivity checks and select a working pair.
    ///
    /// This function iterates through sorted candidate pairs and simulates
    /// connectivity checks. It selects the first pair that succeeds.
    pub fn start_connectivity_checks(&mut self, socket: &std::net::UdpSocket) -> Result<(), Error> {
        if self.candidate_pairs.is_empty() {
            return Err(Error::NoCandidatePairs);
        }

        self.candidate_pairs
            .sort_by(|a, b| b.priority.cmp(&a.priority));

        let mut selected_index = None;

        socket
            .set_read_timeout(Some(Duration::from_millis(300)))
            .map_err(|_| Error::NetworkInterfaceError)?; // O el error que prefieras

        for (index, pair) in self.candidate_pairs.iter_mut().enumerate() {
            if pair.local.candidate_type == CandidateType::ServerReflexive
                || pair.remote.candidate_type == CandidateType::ServerReflexive
            {
                self.logger
                    .debug(&format!("[ICE] Skipping STUN (srflx) pair: {}", pair));
                continue;
            }

            pair.state = ConnectivityState::InProgress;
            self.logger.debug(&format!(
                "[ICE] Checking pair: {} <-> {}",
                pair.local.address, pair.remote.address
            ));

            let target = format!("{}:{}", pair.remote.address, pair.remote.port);

            if Self::verify_candidate_pair(socket, &target, &self.logger) {
                pair.state = ConnectivityState::Succeeded;
                self.logger.info(&format!("[ICE] Pair VALIDATED: {}", pair));
                selected_index = Some(index);
                break;
            } else {
                pair.state = ConnectivityState::Failed;
                self.logger.debug(&format!("[ICE] Pair FAILED: {}", pair));
            }
        }

        let _ = socket.set_read_timeout(None);

        if let Some(index) = selected_index {
            self.selected_pair = Some(self.candidate_pairs[index].clone());
            Ok(())
        } else {
            Err(Error::NoSelectedPair)
        }
    }

    /// Verify that a candidate pair is reachable via PING/PONG exchange.
    ///
    /// Performs a connectivity check by sending PING messages to the target
    /// address and waiting for a PONG response. The check repeats for up to 2 seconds.
    /// If a PING is received from the remote, a PONG is sent back in response.
    ///
    /// # Arguments
    ///
    /// - `socket`: the UDP socket to use for sending and receiving messages.
    /// - `target`: the target address in "IP:port" format to send PING messages to.
    ///
    /// # Returns
    ///
    /// - `true` if a valid PONG response is received from the target within the timeout.
    /// - `false` if no PONG is received or the timeout expires.
    fn verify_candidate_pair(socket: &std::net::UdpSocket, target: &str, logger: &Logger) -> bool {
        let start = Instant::now();
        let max_duration = Duration::from_secs(2);
        let mut buf = [0u8; 1024];

        while start.elapsed() < max_duration {
            if let Err(e) = socket.send_to(b"PING", target) {
                logger.warn(&format!("[ICE] Send error: {}", e));
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            match socket.recv_from(&mut buf) {
                Ok((amt, src)) => {
                    if !target.contains(&src.ip().to_string()) {
                        continue;
                    }

                    let msg = &buf[..amt];

                    if msg == b"PONG" {
                        logger.debug(&format!("[ICE] Received PONG from remote: {}", src));
                        return true;
                    } else if msg == b"PING" {
                        let _ = socket.send_to(b"PONG", target);
                        logger.debug("[ICE] Received PING from remote, sent PONG back.");
                    }
                }
                Err(_) => {
                    continue;
                }
            }
        }

        false
    }

    /// Returns the first non-loopback local IPv4 address found on the host.
    ///
    /// This is the same source used while gathering host ICE candidates.
    pub fn get_local_ip_str(&self) -> Result<String, Error> {
        get_local_ip()
    }

    /// Return a reference to the first local candidate, if any.
    #[must_use]
    pub fn get_local_candidate(&self) -> Option<&Candidate> {
        self.local_candidates.first()
    }

    #[must_use]
    pub fn get_local_candidates(&self) -> &[Candidate] {
        &self.local_candidates
    }

    /// Returns the selected candidate pair used for data transmission.
    ///
    /// Returns `Error::NoSelectedPair` if connectivity checks have not selected
    /// a pair yet.
    pub fn get_selected_pair(&self) -> Result<&CandidatePair, Error> {
        self.selected_pair.as_ref().ok_or(Error::NoSelectedPair)
    }

    /// Resets remote ICE state.
    ///
    /// Clears remote candidates, candidate pairs, and any previously selected
    /// pair so the agent can be reused for a new negotiation.
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
    use crate::session::ice::connectivity_state::ConnectivityState;
    use std::net::UdpSocket;
    use std::thread;
    use std::time::Duration;

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

    fn build_remote_candidate(ip: String, port: u16) -> Candidate {
        Candidate::new(
            CandidateType::Host,
            126,
            ip,
            port,
            1,
            "1".to_string(),
            "udp".to_string(),
        )
    }

    #[test]
    fn test_new_agent_is_empty() {
        let logger = Logger::new("/dev/null").expect("Failed to create logger");
        let agent = IceAgent::new(logger);

        assert!(agent.local_candidates.is_empty());
        assert!(agent.remote_candidates.is_empty());
        assert!(agent.candidate_pairs.is_empty());
        assert!(agent.get_selected_pair().is_err());
    }

    #[test]
    fn test_gather_candidates_adds_local_candidate() {
        let logger = Logger::new("/dev/null").expect("Failed to create logger");
        let mut agent = IceAgent::new(logger);

        let socket = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind test socket");
        let assigned_port = socket
            .local_addr()
            .expect("Failed to get socket local address")
            .port();

        let result = agent.gather_candidates(&socket, &make_ice_config());

        if matches!(
            result,
            Err(Error::NetworkInterfaceError | Error::NoNetworkInterfaceFound)
        ) {
            return;
        }
        assert!(result.is_ok());

        assert!(!agent.local_candidates.is_empty());

        let host_candidate = agent
            .local_candidates
            .iter()
            .find(|c| c.candidate_type == CandidateType::Host)
            .expect("Host candidate should exist after gather_candidates");

        assert_eq!(host_candidate.port, assigned_port);
    }

    #[test]
    fn test_add_remote_candidate_creates_pairs() -> Result<(), Error> {
        let logger = Logger::new("/dev/null").expect("Failed to create logger");
        let mut agent = IceAgent::new(logger);

        let socket = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind test socket");
        agent.gather_candidates(&socket, &make_ice_config())?;

        let remote = build_remote_candidate("127.0.0.1".to_string(), 9000);

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
        let logger = Logger::new("/dev/null").expect("Failed to create logger");
        let mut agent = IceAgent::new(logger);

        let local_socket = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind local socket");
        agent.gather_candidates(&local_socket, &make_ice_config())?;

        let remote_socket = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind remote socket");
        let remote_addr = remote_socket
            .local_addr()
            .expect("Failed to get remote socket address");

        thread::spawn(move || {
            let mut buf = [0u8; 1024];
            let _ = remote_socket.set_read_timeout(Some(Duration::from_secs(2)));

            while let Ok((_amt, _src)) = remote_socket.recv_from(&mut buf) {
                if let Ok((amt, src)) = remote_socket.recv_from(&mut buf) {
                    let msg = &buf[..amt];
                    if msg == b"PING" {
                        let _ = remote_socket.send_to(b"PONG", src);
                        break;
                    }
                } else {
                    break;
                }
            }
        });

        let remote_candidate =
            build_remote_candidate(remote_addr.ip().to_string(), remote_addr.port());
        agent.add_remote_candidate(remote_candidate)?;

        let result = agent.start_connectivity_checks(&local_socket);
        assert!(result.is_ok());

        let selected = agent.get_selected_pair()?;
        assert_eq!(selected.state, ConnectivityState::Succeeded);

        assert_eq!(selected.remote.port, remote_addr.port());

        Ok(())
    }

    #[test]
    fn test_error_when_starting_checks_without_pairs() {
        let logger = Logger::new("/dev/null").expect("Failed to create logger");
        let mut agent = IceAgent::new(logger);

        let dummy_socket = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind socket");

        let result = agent.start_connectivity_checks(&dummy_socket);

        assert!(matches!(result, Err(Error::NoCandidatePairs)));
    }
}
