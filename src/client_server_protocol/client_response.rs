use crate::session::sdp::SessionDescriptionProtocol;

pub enum ClientResponse {
    Username(String),

    CallAccept {
        sdp_answer: SessionDescriptionProtocol,
    },

    CallReject,
}

impl ClientResponse {
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let s = match self {
            Self::Username(s) => format!("USERNAME|{s}"),
            Self::CallAccept { sdp_answer } => format!("CALLACC|{sdp_answer}"),

            Self::CallReject => "CALLREJ".to_string(),
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
            "CALLACC" if parts.len() == 2 => Some(Self::CallAccept {
                sdp_answer: parts[1].to_string().parse().ok()?,
            }),

            "CALLREJ" if parts.len() == 1 => Some(Self::CallReject),
            _ => None,
        }
    }
}
