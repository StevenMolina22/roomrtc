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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ice::candidate_type::CandidateType;

    #[test]
    fn new_candidate_fields_set_correctly() {
        let candidate = Candidate::new(
            CandidateType::ServerReflexive,
            123456,
            "10.0.0.5".to_string(),
            3478,
            1,
            "foundation-xyz".to_string(),
            "TCP".to_string(),
        );

        assert_eq!(candidate.candidate_type, CandidateType::ServerReflexive);
        assert_eq!(candidate.priority, 123456);
        assert_eq!(candidate.address, "10.0.0.5");
        assert_eq!(candidate.port, 3478);
        assert_eq!(candidate.component_id, 1);
        assert_eq!(candidate.foundation, "foundation-xyz");
        assert_eq!(candidate.transport, "TCP");
    }

    #[test]
    fn new_host_sets_correct_values() {
        let candidate = Candidate::new_host("192.168.0.10".to_string(), 2);

        assert_eq!(candidate.candidate_type, CandidateType::Host);
        assert_eq!(candidate.address, "192.168.0.10");
        assert_eq!(candidate.port, 0); // por default en host
        assert_eq!(candidate.component_id, 2);
        assert_eq!(candidate.foundation, "1");
        assert_eq!(candidate.transport, "UDP");

        // validate priority formula:
        // Host => type_pref = 126
        // local_pref = 65535
        // priority = (126 << 24) | (65535 << 8) | 255
        let expected_priority = (126u32 << 24) | (u32::from(65535u16) << 8) | (256 - 1);
        assert_eq!(candidate.priority, expected_priority);
    }

    #[test]
    fn calculate_priority_for_host() {
        // test indirectly through new_host
        let candidate = Candidate::new_host("1.1.1.1".to_string(), 1);
        let priority = candidate.priority;

        let expected = (126u32 << 24) | (u32::from(65535u16) << 8) | (256 - 1);

        assert_eq!(priority, expected);
    }

    #[test]
    fn display_format() {
        let candidate = Candidate::new(
            CandidateType::Host,
            999999,
            "8.8.8.8".to_string(),
            1234,
            1,
            "test".to_string(),
            "UDP".to_string(),
        );

        let display = format!("{}", candidate);

        assert!(display.contains("8.8.8.8"));
        assert!(display.contains("1234"));
        assert!(display.contains("host"));
        assert!(display.contains("999999"));
    }
}
