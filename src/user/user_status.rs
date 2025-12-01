use std::fmt::Display;

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum UserStatus {
    Available,
    Occupied(String),
    Offline,
}

impl UserStatus {
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
            Self::Occupied(some) => write!(f, "Occupied:{some}"),
            Self::Offline => write!(f, "Offline"),
        }
    }
}
