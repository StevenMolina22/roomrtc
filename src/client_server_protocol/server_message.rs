use crate::session::sdp::SessionDescriptionProtocol;
use crate::user::UserStatus;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ServerMessage {
    UsernameRequest,

    CallIncoming {
        from_usr: String,
        offer_sdp: SessionDescriptionProtocol,
    },

    CallAccepted {
        from_usr: String,
        sdp_answer: SessionDescriptionProtocol,
    },

    CallRejected,

    UserStatusUpdate(String, UserStatus),

    Error(String),
}

impl ServerMessage {
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let s = match self {
            Self::UsernameRequest => "USERNAMEREQUEST".to_string(),
            
            Self::CallIncoming { from_usr, offer_sdp } => 
                format!("CALLINCOMING|{from_usr}|{offer_sdp}"),
            
            Self::CallAccepted { from_usr, sdp_answer } => 
                format!("CALLACCEPTED|{from_usr}|{sdp_answer}"),
            
            Self::CallRejected => "CALLREJECTED".to_string(),
            
            Self::UserStatusUpdate(user, status) => format!("USERSTATE|{user}|{status}"),
            
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
            "USERNAMEREQUEST" if parts.len() == 1 => Some(Self::UsernameRequest),
            
            "CALLINCOMING" if parts.len() == 3 => Some(Self::CallIncoming {
                from_usr: parts[1].into(),
                offer_sdp: parts[2].to_string().parse().ok()?,
            }),
            
            "CALLACCEPTED" if parts.len() == 3 => Some(Self::CallAccepted {
                from_usr: parts[1].to_string(),
                sdp_answer: parts[2].parse().ok()?,
            }),
            
            "CALLREJECTED" if parts.len() == 1 => Some(Self::CallRejected),
            
            "USERSTATE" if parts.len() == 3 => Some(Self::UserStatusUpdate(
                parts[1].into(),
                UserStatus::from_bytes(parts[2].as_ref())?,
            )),
            
            "ERROR" if parts.len() == 2 => Some(Self::Error(parts[1].into())),


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
