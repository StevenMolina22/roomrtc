use crate::user::UserStatus;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;

/// Represents responses sent by the server to client requests.
///
/// This enum encompasses all possible responses to various client operations,
/// including authentication, call management, and error reporting.
///
/// # Variants
///
/// - `LoginOk`: Successful login with token and user list.
/// - `LoginError`: Failed login with error message.
/// - `SignupOk`: Successful user registration.
/// - `SignupError`: Failed registration with error message.
/// - `LogoutOk`: Successful logout.
/// - `LogoutError`: Failed logout with error message.
/// - `CallHangUpOk`: Successful call termination.
/// - `CallHangUpError`: Failed call termination with error message.
/// - `CallRequestOk`: Call request sent successfully.
/// - `CallRequestError`: Failed call request with error message.
/// - `CallAcceptOk`: Call acceptance sent successfully.
/// - `CallAcceptError`: Failed call acceptance with error message.
/// - `CallRejectOk`: Call rejection sent successfully.
/// - `CallRejectError`: Failed call rejection with error message.
/// - `BadMessage`: Received message could not be processed.
/// - `Error`: General error message.
#[derive(Debug)]
pub enum ServerResponse {
    Welcome,
    ServerFull,

    /// Successful login response with authentication token, server address, and list of online users.
    ///
    /// # Fields
    ///
    /// * Token for future authenticated requests
    /// * Server socket address for the session
    /// * HashMap of username to UserStatus for all connected users
    LoginOk(String, SocketAddr, HashMap<String, UserStatus>),

    /// Login failure response with error details.
    LoginError(String),

    /// Successful user registration.
    SignupOk,

    /// Sign up failure response with error details.
    SignupError(String),

    /// Successful logout.
    LogoutOk,

    /// Logout failure response with error details.
    LogoutError(String),

    /// Successful call hang up.
    CallHangUpOk,

    /// Call hang up failure response with error details.
    CallHangUpError(String),

    /// Successful call request transmission.
    CallRequestOk,

    /// Call request failure response with error details.
    CallRequestError(String),

    /// Successful call acceptance transmission.
    CallAcceptOk,

    /// Call acceptance failure response with error details.
    CallAcceptError(String),

    /// Successful call rejection transmission.
    CallRejectOk,

    /// Call rejection failure response with error details.
    CallRejectError(String),

    /// Malformed message received.
    BadMessage,

    /// General error response.
    Error(String),
}

impl ServerResponse {
    /// Converts a server response to its byte representation.
    ///
    /// This method serializes the response to a pipe-delimited text format
    /// and appends a newline at the end for network transmission.
    ///
    /// # Protocol Format
    ///
    /// - LoginOk: `LOGINOK|token|address|user1,status1;user2,status2;...\n`
    /// - LoginError: `LOGINERROR|message\n`
    /// - SignupOk: `SIGNUPOK\n`
    /// - SignupError: `SIGNUPERROR|message\n`
    /// - LogoutOk: `LOGOUTOK\n`
    /// - LogoutError: `LOGOUTERROR|message\n`
    /// - CallHangUpOk: `CALLHANGUPOK\n`
    /// - CallHangUpError: `CALLHANGUPERROR|message\n`
    /// - CallRequestOk: `CALLREQUESTOK\n`
    /// - CallRequestError: `CALLREQUESTERROR|message\n`
    /// - CallAcceptOk: `CALLACCEPTOK\n`
    /// - CallAcceptError: `CALLACCEPTERROR|message\n`
    /// - CallRejectOk: `CALLREJECTOK\n`
    /// - CallRejectError: `CALLREJECTERROR|message\n`
    /// - BadMessage: `BADMSG\n`
    /// - Error: `ERROR|message\n`
    ///
    /// # Returns
    ///
    /// A vector of bytes containing the serialized response.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let s = match self {
            Self::Welcome => "WELCOME".to_string(),
            Self::ServerFull => "SERVERFULL".to_string(),
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

            Self::CallRequestOk => "CALLREQUESTOK".to_string(),

            Self::CallRequestError(msg) => format!("CALLREQUESTERROR|{msg}"),

            Self::CallAcceptOk => "CALLACCEPTOK".to_string(),

            Self::CallAcceptError(msg) => format!("CALLACCEPTERROR|{msg}"),

            Self::CallRejectOk => "CALLREJECTOK".to_string(),

            Self::CallRejectError(msg) => format!("CALLREJECTERROR|{msg}"),

            Self::BadMessage => "BADMSG".to_string(),

            Self::Error(msg) => format!("ERROR|{msg}"),
        };

        let mut bytes = s.into_bytes();
        bytes.push(b'\n');
        bytes
    }

    /// Deserializes a server response from bytes.
    ///
    /// This method parses a byte representation of the response (in pipe-delimited text format)
    /// and constructs the corresponding `ServerResponse` variant.
    ///
    /// # Parameters
    ///
    /// * `bytes` - Byte slice containing the serialized response. Expected to be valid UTF-8
    ///   and terminated with a newline character.
    ///
    /// # Returns
    ///
    /// * `Some(ServerResponse)` - If deserialization is successful.
    /// * `None` - If bytes are not valid UTF-8, format is incorrect,
    ///   or there are insufficient fields for the specified variant.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(bytes).ok()?.trim();
        let parts: Vec<&str> = s.split('|').collect();

        match parts[0] {
            "WELCOME" => Some(Self::Welcome),
            "SERVERFULL" => Some(Self::ServerFull),

            "LOGINOK" if parts.len() >= 4 => {
                let token = parts[1].to_string();
                let address = SocketAddr::from_str(parts[2]).ok()?;
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

            "SIGNUPOK" if parts.len() == 1 => Some(Self::SignupOk),

            "SIGNUPERROR" if parts.len() == 2 => Some(Self::SignupError(parts[1].into())),

            "LOGOUTOK" if parts.len() == 1 => Some(Self::LogoutOk),

            "LOGOUTERROR" if parts.len() == 2 => Some(Self::LogoutError(parts[1].into())),

            "CALLHANGUPOK" if parts.len() == 1 => Some(Self::CallHangUpOk),

            "CALLHANGUPERROR" if parts.len() == 2 => Some(Self::CallHangUpError(parts[1].into())),

            "CALLREQUESTOK" if parts.len() == 1 => Some(Self::CallRequestOk),

            "CALLREQUESTERROR" if parts.len() == 2 => Some(Self::CallRequestError(parts[1].into())),

            "CALLACCEPTOK" if parts.len() == 1 => Some(Self::CallAcceptOk),

            "CALLACCEPTERROR" if parts.len() == 2 => Some(Self::CallAcceptError(parts[1].into())),

            "CALLREJECTOK" if parts.len() == 1 => Some(Self::CallRejectOk),

            "CALLREJECTERROR" if parts.len() == 2 => Some(Self::CallRejectError(parts[1].into())),

            // "SERVERFULL" if parts.len() == 1 => Some(Self::ServerFull),
            "BADMSG" if parts.len() == 1 => Some(Self::BadMessage),

            "ERROR" if parts.len() == 2 => Some(Self::Error(parts[1].into())),

            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user::UserStatus;
    use std::collections::HashMap;
    use std::net::SocketAddr;

    #[test]
    fn test_welcome_and_simple_variants() {
        let welcome = ServerResponse::Welcome;
        let bytes = welcome.to_bytes();
        assert_eq!(bytes, b"WELCOME\n");
        assert!(matches!(
            ServerResponse::from_bytes(&bytes),
            Some(ServerResponse::Welcome)
        ));

        let bad_msg = ServerResponse::BadMessage;
        assert_eq!(bad_msg.to_bytes(), b"BADMSG\n");
    }

    #[test]
    fn test_login_ok_serialization() {
        let mut users = HashMap::new();
        users.insert("alice".to_string(), UserStatus::Available);
        users.insert("bob".to_string(), UserStatus::Offline);

        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let response = ServerResponse::LoginOk("test_token".into(), addr, users);

        let bytes = response.to_bytes();
        let deserialized = ServerResponse::from_bytes(&bytes).expect("Should deserialize LoginOk");

        if let ServerResponse::LoginOk(token, address, user_map) = deserialized {
            assert_eq!(token, "test_token");
            assert_eq!(address, addr);
            assert_eq!(user_map.get("alice"), Some(&UserStatus::Available));
            assert_eq!(user_map.get("bob"), Some(&UserStatus::Offline));
            assert_eq!(user_map.len(), 2);
        } else {
            panic!("Expected LoginOk variant");
        }
    }

    #[test]
    fn test_login_ok_empty_users() {
        let addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let response = ServerResponse::LoginOk("token".into(), addr, HashMap::new());

        let bytes = response.to_bytes();
        let deserialized = ServerResponse::from_bytes(&bytes).unwrap();

        if let ServerResponse::LoginOk(_, _, user_map) = deserialized {
            assert!(user_map.is_empty());
        } else {
            panic!("Expected LoginOk variant");
        }
    }

    #[test]
    fn test_error_variants() {
        let cases = vec![
            (
                ServerResponse::LoginError("invalid pass".into()),
                "LOGINERROR|invalid pass\n",
            ),
            (
                ServerResponse::SignupError("user exists".into()),
                "SIGNUPERROR|user exists\n",
            ),
            (
                ServerResponse::Error("fatal crash".into()),
                "ERROR|fatal crash\n",
            ),
        ];

        for (resp, expected_str) in cases {
            let bytes = resp.to_bytes();
            assert_eq!(bytes, expected_str.as_bytes());

            let deserialized = ServerResponse::from_bytes(&bytes).unwrap();
            match (resp, deserialized) {
                (ServerResponse::Error(a), ServerResponse::Error(b)) => assert_eq!(a, b),
                (ServerResponse::LoginError(a), ServerResponse::LoginError(b)) => assert_eq!(a, b),
                (ServerResponse::SignupError(a), ServerResponse::SignupError(b)) => {
                    assert_eq!(a, b)
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_malformed_login_ok() {
        let bad_addr = b"LOGINOK|token|not_an_ip|user,status\n";
        assert!(ServerResponse::from_bytes(bad_addr).is_none());

        let missing_fields = b"LOGINOK|token|127.0.0.1:80\n";
        assert!(ServerResponse::from_bytes(missing_fields).is_none());
    }

    #[test]
    fn test_call_responses() {
        let resp = ServerResponse::CallRequestOk;
        assert_eq!(resp.to_bytes(), b"CALLREQUESTOK\n");
        assert!(matches!(
            ServerResponse::from_bytes(b"CALLREQUESTOK\n"),
            Some(ServerResponse::CallRequestOk)
        ));

        let resp_err = ServerResponse::CallAcceptError("timeout".into());
        assert_eq!(resp_err.to_bytes(), b"CALLACCEPTERROR|timeout\n");
    }
}
