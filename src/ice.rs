use std::str::FromStr;

#[derive(Clone)]
pub enum CandidateType {
    Host,
    ServerReflexive,
}

impl CandidateType {
    /// Returns the candidate type priority according to RFC 8445
    pub fn priority(&self) -> u32 {
        match self {
            CandidateType::Host => 126,
            CandidateType::ServerReflexive => 100,
        }
    }
}

impl std::fmt::Display for CandidateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CandidateType::Host => write!(f, "host"),
            CandidateType::ServerReflexive => write!(f, "srflx"),
        }
    }
}

impl FromStr for CandidateType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "host" => Ok(CandidateType::Host),
            "srflx" => Ok(CandidateType::ServerReflexive),
            _ => Err(()),
        }
    }
}

// Represents an ICE candidate
#[derive(Clone)]
pub struct Candidate {
    pub candidate_type: CandidateType,
    pub priority: u32,
    pub address: String,
    pub port: u16,
    pub component_id: u8,
    pub foundation: String,
    pub transport: String,
}

impl Candidate {
    pub fn new(candidate_type: CandidateType, priority: u32, address: String, port: u16, component_id: u8, foundation: String, transport:String) -> Self {
        Self {
            candidate_type,
            priority,
            address,
            port,
            component_id,
            foundation,
            transport
        }
    }

    /// Creates a new Host type candidate with the specified IP address
    pub fn new_host(address: String, component_id: u8) -> Self {
        let candidate_type = CandidateType::Host;
        let priority = Self::calculate_priority(&candidate_type, 65535); // High local preference
        let foundation = "1".to_string();
        let transport = "UDP".to_string();

        Candidate {
            candidate_type,
            priority,
            address,
            port: 0,
            component_id,
            foundation,
            transport
        }
    }

    fn calculate_priority(candidate_type: &CandidateType, local_preference: u16) -> u32 {
        let type_preference = match candidate_type {
            CandidateType::Host => 126,
            CandidateType::ServerReflexive => 100,
        };
        (type_preference << 24) | ((local_preference as u32) << 8) | (256 - 1)
    }
}

impl std::fmt::Display for Candidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}@{} ({})",
            self.address, self.port, self.candidate_type, self.priority
        )
    }
}

#[derive(Clone)]
pub enum ConnectivityState {
    Waiting,
    InProgress,
    Succeeded,
    Failed,
}

impl std::fmt::Display for ConnectivityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectivityState::Waiting => write!(f, "Waiting"),
            ConnectivityState::InProgress => write!(f, "In Progress"),
            ConnectivityState::Succeeded => write!(f, "Succeeded"),
            ConnectivityState::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Clone)]
pub struct CandidatePair {
    pub local: Candidate,
    pub remote: Candidate,
    pub priority: u64,
    pub state: ConnectivityState,
}

impl CandidatePair {
    pub fn new(local: Candidate, remote: Candidate) -> Self {
        let priority = Self::calculate_pair_priority(&local, &remote);
        CandidatePair {
            local,
            remote,
            priority,
            state: ConnectivityState::Waiting,
        }
    }

    fn calculate_pair_priority(local: &Candidate, remote: &Candidate) -> u64 {
        let g = std::cmp::min(local.priority, remote.priority) as u64;
        let l = std::cmp::max(local.priority, remote.priority) as u64;
        (1u64 << 32) * g
            + 2 * l
            + if local.priority > remote.priority {
                1
            } else {
                0
            }
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
    pub fn new() -> Self {
        IceAgent {
            local_candidates: Vec::new(),
            remote_candidates: Vec::new(),
            candidate_pairs: Vec::new(),
            selected_pair: None,
        }
    }

    /// Gathers local candidates (available IP addresses)
    /// This implementation only gets the local IP
    pub fn gather_candidates(&mut self, port: u16) -> Result<(), String> {
        // Find my local IP and create a candidate
        let local_ip = self.get_local_ip()?;
        let mut candidate = Candidate::new_host(local_ip, 1);
        candidate.port = port;
        self.local_candidates.push(candidate);

        // In a real ICE implementation, here would be STUN/TURN interaction (final delivery)
        Ok(())
    }
    /// Gets the local IP using if_addrs
    fn get_local_ip(&self) -> Result<String, String> {
        // Get all network interfaces
        // returns a list with Interface type objects
        // each Interface has name, addr, index, oper_status and is_loopback() method
        let interfaces = if_addrs::get_if_addrs()
            .map_err(|e| format!("Error getting interfaces: {}", e))?;
        
        // Find the first interface that is not loopback
        // loopback is a virtual internal interface of the operating system (doesn't connect any local network)
        for interface in interfaces {
            if !interface.is_loopback() {
                return Ok(interface.addr.ip().to_string());
            }
        }
        
        Err("No network interface found".to_string())
    }

    /// Adds a single remote candidate and creates pairs
    pub fn add_remote_candidate(&mut self, candidate: Candidate) -> Result<(), String> {
        self.remote_candidates.push(candidate);
        self.create_candidate_pair()
    }

    /// Adds remote candidates received from the other peer and creates candidate pairs
    pub fn add_remote_candidates(&mut self, candidates: Vec<Candidate>) -> Result<(), String> {
        for candidate in candidates {
            self.remote_candidates.push(candidate);
        }
        self.create_candidate_pair()
    }

    fn create_candidate_pair(&mut self) -> Result<(), String> {
        if self.local_candidates.is_empty() || self.remote_candidates.is_empty() {
            return Err("No candidates available to create a pair".into());
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
    pub fn start_connectivity_checks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.candidate_pairs.is_empty() {
            return Err("No candidate pairs to verify".into());
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

    pub fn get_local_candidate(&self) -> Option<&Candidate> {
        self.local_candidates.first()
    }

    pub fn get_selected_pair(&self) -> Option<&CandidatePair> {
        self.selected_pair.as_ref()
    }
}
