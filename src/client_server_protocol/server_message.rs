use crate::session::sdp::SessionDescriptionProtocol;
use crate::user::UserStatus;

/// Represents different types of messages that the server can send to clients.
///
/// This enum encapsulates all possible messages in the server-to-client communication,
/// including call events, user status updates, and error handling.
///
/// # Variants
///
/// - `UsernameRequest`: Requests the client to provide its username.
/// - `CallIncoming`: Notifies about an incoming call with SDP offer.
/// - `CallAccepted`: Notifies that a call has been accepted with SDP answer.
/// - `CallRejected`: Notifies that a call has been rejected.
/// - `UserStatusUpdate`: Updates client about a user's status change.
/// - `Error`: Communicates an error message to the client.
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ServerMessage {
    /// Request for the client to provide its username.
    UsernameRequest,

    /// Notification of an incoming call.
    ///
    /// # Fields
    ///
    /// * `from_usr` - Username of the caller.
    /// * `offer_sdp` - Session description containing the SDP offer.
    CallIncoming {
        from_usr: String,
        offer_sdp: SessionDescriptionProtocol,
    },

    /// Notification that a call has been accepted.
    ///
    /// # Fields
    ///
    /// * `from_usr` - Username of the user who accepted the call.
    /// * `sdp_answer` - Session description containing the SDP answer.
    CallAccepted {
        from_usr: String,
        sdp_answer: SessionDescriptionProtocol,
    },

    /// Notification that a call has been rejected.
    CallRejected,

    /// Update about a user's online status.
    ///
    /// Contains a tuple of (username, UserStatus).
    UserStatusUpdate(String, UserStatus),

    /// Error message to communicate failures to the client.
    Error(String),
}

impl ServerMessage {
    /// Converts a server message to its byte representation.
    ///
    /// This method serializes the message to a pipe-delimited text format
    /// and appends a newline at the end for network transmission.
    ///
    /// # Protocol Format
    ///
    /// - UsernameRequest: `USERNAMEREQUEST\n`
    /// - CallIncoming: `CALLINCOMING|from_usr|sdp\n`
    /// - CallAccepted: `CALLACCEPTED|from_usr|sdp\n`
    /// - CallRejected: `CALLREJECTED\n`
    /// - UserStatusUpdate: `USERSTATE|username|status\n`
    /// - Error: `ERROR|message\n`
    ///
    /// # Returns
    ///
    /// A vector of bytes containing the serialized message.
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

    /// Deserializes a server message from bytes.
    ///
    /// This method parses a byte representation of the message (in pipe-delimited text format)
    /// and constructs the corresponding `ServerMessage` variant.
    ///
    /// # Parameters
    ///
    /// * `bytes` - Byte slice containing the serialized message. Expected to be valid UTF-8
    ///   and terminated with a newline character.
    ///
    /// # Returns
    ///
    /// * `Some(ServerMessage)` - If deserialization is successful.
    /// * `None` - If bytes are not valid UTF-8, format is incorrect,
    ///   or there are insufficient fields for the specified variant.
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
    

    // Helper function that serializes and deserializes a message to test encoding/decoding
    fn roundtrip(msg: ServerMessage) -> Option<ServerMessage> {
        let bytes = msg.to_bytes();
        ServerMessage::from_bytes(&bytes)
    }

    #[test]
    fn test_username_request() {
        let msg = ServerMessage::UsernameRequest;
        let decoded = roundtrip(msg.clone()).expect("roundtrip failed");
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_error_message() {
        let msg = ServerMessage::Error("SomethingBad".into());
        let decoded = roundtrip(msg.clone()).expect("roundtrip failed");
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
        let decoded = ServerMessage::from_bytes(&encoded).expect("failed to decode");

        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_userstate_offline() {
        let status = UserStatus::Offline;
        let msg = ServerMessage::UserStatusUpdate("Charlie".into(), status);

        let encoded = msg.to_bytes();
        let decoded = ServerMessage::from_bytes(&encoded).expect("failed to decode");

        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_userstate_occupied() {
        let status = UserStatus::Occupied("OnCall".into());
        let msg = ServerMessage::UserStatusUpdate("Dave".into(), status);

        let encoded = msg.to_bytes();
        let decoded = ServerMessage::from_bytes(&encoded).expect("failed to decode");

        assert_eq!(decoded, msg);
    }
}
