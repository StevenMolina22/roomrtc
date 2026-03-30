use crate::session::sdp::SessionDescriptionProtocol;

/// Represents the different types of messages that a client can send to the server.
///
/// This enum encapsulates all possible messages in the client-server protocol,
/// including authentication, call management, and session control.
///
/// # Variants
///
/// - `LogIn`: Attempts to log in with user credentials.
/// - `SignUp`: Registers a new user with credentials.
/// - `LogOut`: Closes a user session.
/// - `CallRequest`: Initiates a call by sending an SDP offer.
/// - `CallAccept`: Accepts an incoming call by sending an SDP answer.
/// - `CallReject`: Rejects an incoming call.
/// - `CallHangup`: Terminates an active call.
pub enum ClientMessage {
    Hello,

    /// Request to log in.
    ///
    /// # Fields
    ///
    /// * `username` - Username for authentication.
    /// * `password` - Password of the user.
    LogIn {
        username: String,
        password: String,
    },

    /// Request to sign up a new user.
    ///
    /// # Fields
    ///
    /// * `username` - Username for the new account.
    /// * `password` - Password for the new user.
    SignUp {
        username: String,
        password: String,
    },

    /// Request to log out.
    ///
    /// # Fields
    ///
    /// * `token` - Session token of the authenticated user.
    LogOut {
        token: String,
    },

    /// Request to initiate an RTC call.
    ///
    /// # Fields
    ///
    /// * `token` - Session token of the user making the call.
    /// * `offer_sdp` - Session description containing the SDP offer for the call.
    /// * `to` - Username of the call recipient.
    CallRequest {
        token: String,
        offer_sdp: SessionDescriptionProtocol,
        to: String,
    },

    /// Acceptance of an incoming RTC call.
    ///
    /// # Fields
    ///
    /// * `from_usr` - Username of the user who made the call.
    /// * `to_usr` - Username of the user accepting the call.
    /// * `sdp_answer` - Session description containing the SDP answer.
    CallAccept {
        from_usr: String,
        to_usr: String,
        sdp_answer: SessionDescriptionProtocol,
    },

    /// Rejection of an incoming RTC call.
    ///
    /// # Fields
    ///
    /// * `from_usr` - Username of the user who made the call.
    /// * `to_usr` - Username of the user rejecting the call.
    CallReject {
        from_usr: String,
        to_usr: String,
    },

    /// Termination of an active call.
    ///
    /// # Fields
    ///
    /// * `token` - Session token of the user ending the call.
    CallHangup {
        token: String,
    },
}

impl ClientMessage {
    /// Converts a client message to its byte representation.
    ///
    /// This method serializes the message to a pipe-delimited text format
    /// and appends a newline at the end for easy network transmission.
    ///
    /// # Protocol Format
    ///
    /// Each variant is converted to a specific format:
    /// - LogIn: `LOGIN|username|password\n`
    /// - SignUp: `SIGNUP|username|password\n`
    /// - LogOut: `LOGOUT|token\n`
    /// - CallRequest: `CALLREQUEST|token|sdp|recipient\n`
    /// - CallAccept: `CALLACCEPT|from|to|sdp\n`
    /// - CallReject: `CALLREJECT|from|to\n`
    /// - CallHangup: `CALLHANG|token\n`
    ///
    /// # Returns
    ///
    /// A vector of bytes containing the serialized message.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let s = match self {
            Self::Hello => "HELLO".to_string(),

            Self::LogIn { username, password } => format!("LOGIN|{username}|{password}"),

            Self::SignUp { username, password } => format!("SIGNUP|{username}|{password}"),

            Self::LogOut { token } => format!("LOGOUT|{token}"),

            Self::CallRequest {
                token,
                offer_sdp,
                to,
            } => format!("CALLREQUEST|{token}|{offer_sdp}|{to}"),

            Self::CallAccept {
                from_usr,
                to_usr,
                sdp_answer,
            } => format!("CALLACCEPT|{from_usr}|{to_usr}|{sdp_answer}"),

            Self::CallReject { from_usr, to_usr } => format!("CALLREJECT|{from_usr}|{to_usr}"),

            Self::CallHangup { token } => format!("CALLHANG|{token}"),
        };

        let mut bytes = s.into_bytes();
        bytes.push(b'\n');
        bytes
    }

    /// Deserializes a client message from bytes.
    ///
    /// This method parses a byte representation of the message (in pipe-delimited text format)
    /// and constructs the corresponding `ClientMessage` variant.
    ///
    /// # Parameters
    ///
    /// * `bytes` - Byte slice containing the serialized message. Expected to be valid UTF-8
    ///   and terminated with a newline character.
    ///
    /// # Returns
    ///
    /// * `Some(ClientMessage)` - If deserialization is successful.
    /// * `None` - If bytes are not valid UTF-8, format is incorrect,
    ///   or there are insufficient fields for the specified variant.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let bytes = b"LOGIN|user123|pass456\n";
    /// let msg = ClientMessage::from_bytes(bytes);
    /// assert!(msg.is_some());
    /// ```
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let s = str::from_utf8(bytes).ok()?.trim();
        let parts: Vec<&str> = s.split('|').collect();

        match parts[0] {
            "HELLO" if parts.len() == 1 => Some(Self::Hello),

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

            "CALLREQUEST" if parts.len() == 4 => Some(Self::CallRequest {
                token: parts[1].into(),
                offer_sdp: parts[2].parse().ok()?,
                to: parts[3].into(),
            }),

            "CALLACCEPT" if parts.len() == 4 => Some(Self::CallAccept {
                from_usr: parts[1].into(),
                to_usr: parts[2].into(),
                sdp_answer: parts[3].parse().ok()?,
            }),

            "CALLREJECT" if parts.len() == 3 => Some(Self::CallReject {
                from_usr: parts[1].into(),
                to_usr: parts[2].into(),
            }),

            "CALLHANG" if parts.len() == 2 => Some(Self::CallHangup {
                token: parts[1].into(),
            }),

            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_login_serialization() {
        let msg = ClientMessage::LogIn {
            username: "alice".into(),
            password: "password123".into(),
        };
        let bytes = msg.to_bytes();
        assert_eq!(bytes, b"LOGIN|alice|password123\n");

        let deserialized = ClientMessage::from_bytes(&bytes).unwrap();
        if let ClientMessage::LogIn { username, password } = deserialized {
            assert_eq!(username, "alice");
            assert_eq!(password, "password123");
        } else {
            panic!("Variant mismatch");
        }
    }

    #[test]
    fn test_hello_variant() {
        let msg = ClientMessage::Hello;
        let bytes = msg.to_bytes();
        assert_eq!(bytes, b"HELLO\n");

        let deserialized = ClientMessage::from_bytes(&bytes).unwrap();
        assert!(matches!(deserialized, ClientMessage::Hello));
    }

    #[test]
    fn test_call_request_serialization() {
        let sdp_str = "v=0\no=- 12345 67890 IN IP4 127.0.0.1";
        let msg = ClientMessage::CallRequest {
            token: "secret_token".into(),
            offer_sdp: sdp_str.parse().unwrap(),
            to: "bob".into(),
        };

        let bytes = msg.to_bytes();
        let deserialized = ClientMessage::from_bytes(&bytes).expect("Should parse CallRequest");

        if let ClientMessage::CallRequest { token, to, .. } = deserialized {
            assert_eq!(token, "secret_token");
            assert_eq!(to, "bob");
        } else {
            panic!("Expected CallRequest variant");
        }
    }

    #[test]
    fn test_invalid_messages() {
        assert!(ClientMessage::from_bytes(b"INVALID|data\n").is_none());

        assert!(ClientMessage::from_bytes(b"LOGIN|only_user\n").is_none());

        let invalid_utf8 = vec![0, 159, 146, 150];
        assert!(ClientMessage::from_bytes(&invalid_utf8).is_none());
    }

    #[test]
    fn test_call_reject() {
        let msg = ClientMessage::CallReject {
            from_usr: "alice".into(),
            to_usr: "bob".into(),
        };
        let bytes = msg.to_bytes();
        assert_eq!(bytes, b"CALLREJECT|alice|bob\n");

        let deserialized = ClientMessage::from_bytes(&bytes).unwrap();
        if let ClientMessage::CallReject { from_usr, to_usr } = deserialized {
            assert_eq!(from_usr, "alice");
            assert_eq!(to_usr, "bob");
        } else {
            panic!("Variant mismatch");
        }
    }
}
