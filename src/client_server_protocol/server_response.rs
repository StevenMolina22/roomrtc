use crate::session::sdp::SessionDescriptionProtocol;
use crate::user::UserStatus;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;

#[derive(Debug)]
pub enum ServerResponse {
    LoginOk(String, SocketAddr, HashMap<String, UserStatus>),
    LoginError(String),

    SignupOk,
    SignupError(String),

    LogoutOk,
    LogoutError(String),

    CallHangUpOk,
    CallHangUpError(String),

    CallAccepted {
        sdp_answer: SessionDescriptionProtocol,
    },

    CallRejected,
    
    BadMessage,

    Error(String),
}

impl ServerResponse {
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let s = match self {
            Self::LoginOk(token, address, users_status) => {
                let kv = users_status
                    .iter()
                    .map(|(k, v)| format!("{k},{v}"))
                    .collect::<Vec<_>>()
                    .join(";");

                format!("LOGINOK|{token}|{address}|{kv}")
            }

            Self::LoginError(msg) => format!("LOGINERROR|{msg}"),

            Self::SignupOk => "SIGNUPOK".to_string(),

            Self::SignupError(msg) => format!("SIGNUPERROR|{msg}"),

            Self::LogoutOk => "LOGOUTOK".to_string(),

            Self::LogoutError(msg) => format!("LOGOUTERROR|{msg}"),

            Self::CallHangUpOk => "CALLHANGUPOK".to_string(),

            Self::CallHangUpError(msg) => format!("CALLHANGUPERROR|{msg}"),

            Self::CallAccepted { sdp_answer } => format!("CALLACCEPTED|{sdp_answer}"),

            Self::CallRejected => "CALLREJECTED".to_string(),
            
            Self::BadMessage => "BADMSG".to_string(),

            Self::Error(msg) => format!("ERROR|{msg}"),
        };

        let mut bytes = s.into_bytes();
        bytes.push(b'\n');
        bytes
    }

    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(bytes).ok()?.trim();
        let parts: Vec<&str> = s.split('|').collect();

        match parts[0] {
            "LOGINOK" if parts.len() >= 4 => {
                let token = parts[1].to_string();
                let address = SocketAddr::from_str(parts[2]).unwrap();
                let users_status_list = parts[3];

                let mut users_status = HashMap::new();

                if !users_status_list.is_empty() {
                    for entry in users_status_list.split(';') {
                        if entry.is_empty() {
                            continue;
                        }

                        let mut it = entry.split(',');
                        let username = it.next()?.to_string();
                        let status_str = it.next()?.to_string();
                        let status = UserStatus::from_bytes(status_str.as_bytes())?;

                        users_status.insert(username, status);
                    }
                }

                Some(Self::LoginOk(token, address, users_status))
            }

            "LOGINERROR" if parts.len() == 2 => Some(Self::LoginError(parts[1].into())),

            "SIGNUPOK" => Some(Self::SignupOk),

            "SIGNUPERROR" if parts.len() == 2 => Some(Self::SignupError(parts[1].into())),

            "LOGOUTOK" => Some(Self::LogoutOk),

            "LOGOUTERROR" if parts.len() == 2 => Some(Self::LogoutError(parts[1].into())),

            "CALLHANGUPOK" => Some(Self::CallHangUpOk),

            "CALLHANGUPERROR" if parts.len() == 2 =>
                Some(Self::CallHangUpError(parts[1].into())),

            "CALLACCEPTED" if parts.len() == 2 => Some(Self::CallAccepted {
                sdp_answer: parts[1].parse().ok()?,
            }),

            "CALLREJECTED" if parts.len() == 1 => Some(Self::CallRejected),
            
            "BADMSG" if parts.len() == 1 => Some(Self::BadMessage),

            "ERROR" if parts.len() == 2 => Some(Self::Error(parts[1].into())),

            _ => None,
        }
    }
}
