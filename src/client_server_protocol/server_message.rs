use crate::session::sdp::SessionDescriptionProtocol;
use crate::user::UserStatus;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ServerMessage {
    UsernameRequest,

    CallIncoming {
        from: String,
        offer_sdp: SessionDescriptionProtocol,
    },

    Error(String),

    UserStatusUpdate(String, UserStatus),
}

impl ServerMessage {
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let s = match self {
            Self::UsernameRequest => "USERNAMEREQUEST".to_string(),
            Self::CallIncoming { from, offer_sdp } => {
                format!("CALLINCOMING|{from}|{offer_sdp}")
            }
            Self::Error(msg) => format!("ERROR|{msg}"),
            Self::UserStatusUpdate(user, status) => {
                format!("USERSTATE|{user}|{status}")
            }
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
            "USERNAMEREQUEST" if parts.len() == 1 => Some(Self::UsernameRequest),
            "CALLINCOMING" if parts.len() == 3 => Some(Self::CallIncoming {
                from: parts[1].into(),
                offer_sdp: parts[2].to_string().parse().ok()?,
            }),

            "ERROR" if parts.len() == 2 => Some(Self::Error(parts[1].into())),

            "USERSTATE" if parts.len() == 3 => Some(Self::UserStatusUpdate(
                parts[1].into(),
                UserStatus::from_bytes(parts[2].as_ref())?,
            )),

            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    // Helper: crea un ServerMessage -> bytes -> ServerMessage nuevamente
    fn roundtrip(msg: ServerMessage) -> ServerMessage {
        let bytes = msg.to_bytes();
        ServerMessage::from_bytes(&bytes).expect("failed to decode")
    }

    #[test]
    fn test_username_request() {
        let msg = ServerMessage::UsernameRequest;
        let decoded = roundtrip(msg.clone());
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_error_message() {
        let msg = ServerMessage::Error("SomethingBad".into());
        let decoded = roundtrip(msg.clone());
        assert_eq!(decoded, msg);
    }

    // -----------------------
    // USERSTATUS TESTS
    // -----------------------

    #[test]
    fn test_userstate_available() {
        let status = UserStatus::Available;
        let msg = ServerMessage::UserStatusUpdate("Bob".into(), status);

        let encoded = msg.to_bytes();
        let decoded = ServerMessage::from_bytes(&encoded).unwrap();

        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_userstate_offline() {
        let status = UserStatus::Offline;
        let msg = ServerMessage::UserStatusUpdate("Charlie".into(), status);

        let encoded = msg.to_bytes();
        let decoded = ServerMessage::from_bytes(&encoded).unwrap();

        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_userstate_occupied() {
        let status = UserStatus::Occupied("OnCall".into());
        let msg = ServerMessage::UserStatusUpdate("Dave".into(), status);

        let encoded = msg.to_bytes();
        let decoded = ServerMessage::from_bytes(&encoded).unwrap();

        assert_eq!(decoded, msg);
    }
}
