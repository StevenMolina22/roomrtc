use crate::config::IceConfig;
use crate::ice::candidate_type::CandidateType;

/// An ICE candidate.
///
/// This struct represents an ICE candidate as used in connectivity checks.
/// It contains the candidate type (host, server-reflexive, etc.), the
/// computed priority and the addressing information required to perform
/// connectivity checks (address, port, transport, component id).
#[derive(Clone)]
pub struct Candidate {
    /// The type/category of the candidate (e.g. host, srflx).
    pub candidate_type: CandidateType,

    /// Computed priority for this candidate instance.
    ///
    /// The priority is calculated following the ICE/RFC 8445 formula and
    /// is used to sort and select candidate pairs.
    pub priority: u32,

    /// IP address or hostname of the candidate.
    pub address: String,

    /// Port number for the candidate.
    pub port: u16,

    /// Component ID
    pub component_id: u8,

    /// Foundation string used in ICE to group related candidates.
    pub foundation: String,

    /// Transport protocol ("UDP").
    pub transport: String,
}

impl Candidate {
    /// Create a new `Candidate` with all fields specified.
    ///
    /// # Parameters
    /// - `candidate_type`: the candidate category.
    /// - `priority`: computed priority value.
    /// - `address`: IP address or host name.
    /// - `port`: transport port number.
    /// - `component_id`: component identifier.
    /// - `foundation`: foundation string.
    /// - `transport`: transport protocol string.
    ///
    /// # Returns
    /// A `Candidate` instance containing the provided values.
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

    /// Create a new host candidate with a given IP address and component id.
    ///
    /// The produced candidate uses a high local preference and sets default
    /// values for foundation and transport. The port is initialized to 0.
    #[must_use]
    pub fn new_host(address: String, component_id: u8, ice_config: &IceConfig) -> Self {
        let candidate_type = CandidateType::Host;
        let priority = Self::calculate_priority(
            &candidate_type,
            ice_config.host_local_preference,
            ice_config,
        );
        let foundation = ice_config.foundation.clone();
        let transport = ice_config.transport.clone();

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

    /// Calculate the candidate priority using the ICE formula.
    ///
    /// The formula implemented here follows RFC 8445: combine type
    /// preference, local preference and component id into a single `u32`
    /// priority value.
    fn calculate_priority(
        candidate_type: &CandidateType,
        local_preference: u16,
        ice_config: &IceConfig,
    ) -> u32 {
        let type_preference = match candidate_type {
            CandidateType::Host => ice_config.host_priority_preference,
            CandidateType::ServerReflexive => ice_config.srflx_priority_preference,
        };
        (type_preference << 24) | (u32::from(local_preference) << 8) | (256 - 1)
    }
}

impl std::fmt::Display for Candidate {
    /// Format the candidate for debug/logging: `address:port@type (priority)`.
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

    fn test_ice_config() -> IceConfig {
        IceConfig {
            foundation: "1".to_string(),
            transport: "UDP".to_string(),
            component_id: 1,
            host_priority_preference: 126,
            srflx_priority_preference: 100,
            host_local_preference: 65535,
        }
    }

    #[test]
    fn new_candidate_fields_set_correctly() {
        let candidate = Candidate::new(
            CandidateType::ServerReflexive,
            123_456,
            "10.0.0.5".to_string(),
            3478,
            1,
            "foundation-xyz".to_string(),
            "TCP".to_string(),
        );

        assert_eq!(candidate.candidate_type, CandidateType::ServerReflexive);
        assert_eq!(candidate.priority, 123_456);
        assert_eq!(candidate.address, "10.0.0.5");
        assert_eq!(candidate.port, 3478);
        assert_eq!(candidate.component_id, 1);
        assert_eq!(candidate.foundation, "foundation-xyz");
        assert_eq!(candidate.transport, "TCP");
    }

    #[test]
    fn new_host_sets_correct_values() {
        let candidate = Candidate::new_host("192.168.0.10".to_string(), 2, &test_ice_config());

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
        let candidate = Candidate::new_host("1.1.1.1".to_string(), 1, &test_ice_config());
        let priority = candidate.priority;

        let expected = (126u32 << 24) | (u32::from(65535u16) << 8) | (256 - 1);

        assert_eq!(priority, expected);
    }

    #[test]
    fn display_format() {
        let candidate = Candidate::new(
            CandidateType::Host,
            999_999,
            "8.8.8.8".to_string(),
            1234,
            1,
            "test".to_string(),
            "UDP".to_string(),
        );

        let display = format!("{candidate}");

        assert!(display.contains("8.8.8.8"));
        assert!(display.contains("1234"));
        assert!(display.contains("host"));
        assert!(display.contains("999999"));
    }
}
