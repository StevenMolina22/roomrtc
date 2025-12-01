use crate::session::sdp::SessionDescriptionProtocol;

pub enum ClientMessage {
    LogIn {
        username: String,
        password: String,
    },

    SignUp {
        username: String,
        password: String,
    },

    LogOut {
        token: String,
    },

    CallRequest {
        token: String,
        offer_sdp: SessionDescriptionProtocol,
        to: String,
    },

    CallHangup {
        token: String,
    },
}

impl ClientMessage {
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let s = match self {
            Self::LogIn { username, password } => format!("LOGIN|{username}|{password}"),

            Self::SignUp { username, password } => format!("SIGNUP|{username}|{password}"),

            Self::LogOut { token } => format!("LOGOUT|{token}"),

            Self::CallRequest {
                token,
                offer_sdp,
                to,
            } => format!("CALLREQ|{token}|{offer_sdp}|{to}"),

            Self::CallHangup { token } => format!("CALLHANG|{token}"),
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
            "LOGIN" if parts.len() == 3 => Some(Self::LogIn {
                username: parts[1].into(),
                password: parts[2].into(),
            }),

            "SIGNUP" if parts.len() == 3 => Some(Self::SignUp {
                username: parts[1].into(),
                password: parts[2].into(),
            }),

            "LOGOUT" if parts.len() == 2 => Some(Self::LogOut {
                token: parts[1].into(),
            }),

            "CALLREQ" if parts.len() == 4 => Some(Self::CallRequest {
                token: parts[1].into(),
                offer_sdp: parts[2].parse().ok()?,
                to: parts[3].into(),
            }),

            "CALLHANG" if parts.len() == 2 => Some(Self::CallHangup {
                token: parts[1].into(),
            }),

            _ => None,
        }
    }
}
