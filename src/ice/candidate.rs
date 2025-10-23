use crate::ice::candidate_type::CandidateType;

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
    #[must_use]
    pub const fn new(
        candidate_type: CandidateType,
        priority: u32,
        address: String,
        port: u16,
        component_id: u8,
        foundation: String,
        transport: String,
    ) -> Self {
        Self {
            candidate_type,
            priority,
            address,
            port,
            component_id,
            foundation,
            transport,
        }
    }

    /// Creates a new Host type candidate with the specified IP address
    #[must_use]
    pub fn new_host(address: String, component_id: u8) -> Self {
        let candidate_type = CandidateType::Host;
        let priority = Self::calculate_priority(&candidate_type, 65535); // High local preference
        let foundation = "1".to_string();
        let transport = "UDP".to_string();

        Self {
            candidate_type,
            priority,
            address,
            port: 0,
            component_id,
            foundation,
            transport,
        }
    }

    fn calculate_priority(candidate_type: &CandidateType, local_preference: u16) -> u32 {
        let type_preference = match candidate_type {
            CandidateType::Host => 126,
            CandidateType::ServerReflexive => 100,
        };
        (type_preference << 24) | (u32::from(local_preference) << 8) | (256 - 1)
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
