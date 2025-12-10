pub enum ClientResponse {
    Username(String),
}

impl ClientResponse {
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let s = match self {
            Self::Username(s) => format!("USERNAME|{s}"),
        };

        let mut bytes = s.into_bytes();
        bytes.push(b'\n');
        bytes
    }

    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let s = str::from_utf8(bytes).ok()?.trim();
        let parts: Vec<&str> = s.split('|').collect();

        match parts[0] {
            "USERNAME" if parts.len() == 2 => Some(Self::Username(parts[1].to_string())),
            _ => None,
        }
    }
}
