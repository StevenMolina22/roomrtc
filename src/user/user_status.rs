use std::fmt::Display;

/// Represents the availability status of a user.
///
/// Users can be available for calls, occupied with a description of their current activity,
/// or offline.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum UserStatus {
    /// User is available and can receive calls.
    Available,
    /// User is occupied with a description of their activity (e.g., "In a meeting").
    Occupied(String),
    /// User is offline and unavailable.
    Offline,
}

impl UserStatus {
    /// Parses a byte slice into a `UserStatus`.
    ///
    /// Expected formats:
    /// - `"Available"` -> `UserStatus::Available`
    /// - `"Offline"` -> `UserStatus::Offline`
    /// - `"Occupied:description"` -> `UserStatus::Occupied(description)`
    ///
    /// # Arguments
    /// * `bytes` - A byte slice representing the status.
    ///
    /// # Returns
    /// `Some(UserStatus)` if parsing succeeds, `None` otherwise.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(bytes).ok()?.trim();
        let parts: Vec<&str> = s.split(':').collect();

        match parts[0] {
            "Available" if parts.len() == 1 => Some(Self::Available),
            "Occupied" if parts.len() == 2 => Some(Self::Occupied(parts[1].to_string())),
            "Offline" if parts.len() == 1 => Some(Self::Offline),
            _ => None,
        }
    }
}

impl Display for UserStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Available => write!(f, "Available"),
            Self::Occupied(_) => write!(f, "Occupied"),
            Self::Offline => write!(f, "Offline"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_bytes_available() {
        let input = b"Available";
        let status = UserStatus::from_bytes(input);
        assert_eq!(status, Some(UserStatus::Available));
    }

    #[test]
    fn test_from_bytes_offline() {
        let input = b"Offline";
        let status = UserStatus::from_bytes(input);
        assert_eq!(status, Some(UserStatus::Offline));
    }

    #[test]
    fn test_from_bytes_occupied_valid() {
        let input = b"Occupied:Meeting";
        let status = UserStatus::from_bytes(input);
        assert_eq!(status, Some(UserStatus::Occupied("Meeting".to_string())));
    }

    #[test]
    fn test_from_bytes_occupied_with_spaces() {
        let input = b"Occupied:Coding Hard";
        let status = UserStatus::from_bytes(input);
        assert_eq!(
            status,
            Some(UserStatus::Occupied("Coding Hard".to_string()))
        );
    }

    #[test]
    fn test_from_bytes_invalid_format() {
        let input = b"Occupied";
        assert_eq!(UserStatus::from_bytes(input), None);

        let input = b"Busy";
        assert_eq!(UserStatus::from_bytes(input), None);

        let input = b"Available:Now";
        assert_eq!(UserStatus::from_bytes(input), None);
    }

    #[test]
    fn test_from_bytes_empty() {
        let input = b"";
        assert_eq!(UserStatus::from_bytes(input), None);
    }

    #[test]
    fn test_from_bytes_utf8_error() {
        let input = b"\xff";
        assert_eq!(UserStatus::from_bytes(input), None);
    }

    #[test]
    fn test_display_implementation() {
        assert_eq!(format!("{}", UserStatus::Available), "Available");
        assert_eq!(format!("{}", UserStatus::Offline), "Offline");
        assert_eq!(
            format!("{}", UserStatus::Occupied("Gaming".to_string())),
            "Occupied:Gaming"
        );
    }

    #[test]
    fn test_clone_and_equality() {
        let status = UserStatus::Occupied("Work".to_string());
        let cloned = status.clone();
        assert_eq!(status, cloned);
        assert_ne!(status, UserStatus::Available);
    }
}
