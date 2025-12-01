use super::ServerError;
use crate::client_server_protocol::{ClientResponse, ServerMessage, ServerResponse};
use crate::logger::Logger;
use crate::session::sdp::SessionDescriptionProtocol;
use crate::user::{UserData, UserStatus};
use rustls::{ServerConnection, StreamOwned};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

pub struct OperatingServer {
    users: Arc<RwLock<HashMap<String, UserData>>>,
    users_connected: Arc<AtomicUsize>,
    server_client_socket_address: SocketAddr,
    users_file_path: String,
    username: Option<String>,
    logger: Logger,
}

impl OperatingServer {
    pub const fn new(
        users: Arc<RwLock<HashMap<String, UserData>>>,
        users_connected: Arc<AtomicUsize>,
        server_client_socket_address: SocketAddr,
        users_file_path: String,
        logger: Logger,
    ) -> Self {
        Self {
            users,
            users_connected,
            server_client_socket_address,
            users_file_path,
            username: None,
            logger,
        }
    }
    /// Handles user login logic.
    ///
    /// Validates username and password, checks if the user is not already
    /// logged in, updates the internal user map, and increments the global
    /// connected-user counter.
    ///
    /// Returns a `ServerEvent` describing the outcome.
    pub fn login_user(&mut self, username: String, password: String) -> ServerResponse {
        let status_to_notify;

        {
            let mut users = match self.users.write() {
                Ok(users) => users,
                Err(e) => return ServerResponse::Error(e.to_string()),
            };

            if let Some(data) = users.get_mut(&username) {
                if data.password != password {
                    return ServerResponse::LoginError("Wrong password. Try again".to_string());
                }

                match data.status {
                    UserStatus::Available | UserStatus::Occupied(_) => {
                        return ServerResponse::LoginError("User already logged in".to_string());
                    }

                    UserStatus::Offline => {
                        data.update_status(UserStatus::Available);
                        self.users_connected.fetch_add(1, Ordering::SeqCst);

                        status_to_notify = (username.clone(), UserStatus::Available);
                    }
                }
            } else {
                return ServerResponse::LoginError(format!("User {username} not found"));
            }
        }

        self.username = Some(username.clone());
        notify_status_update(&self.users, status_to_notify.0, status_to_notify.1);
        ServerResponse::LoginOk(
            username.clone(),
            self.server_client_socket_address,
            self.get_clients_for_user(username.clone()),
        )
    }

    /// Registers a new user.
    ///
    /// The username must not already be present in the user map.
    /// New users are always created with `UserStatus::Offline`.
    pub fn signup_user(&mut self, username: String, password: String) -> ServerResponse {
        let status_to_notify;
        if let Err(_) = validate_string(username.clone()) {
            return ServerResponse::SignupError("Invalid username".to_string());
        }
        if let Err(_) = validate_string(password.clone()) {
            return ServerResponse::SignupError("Invalid password".to_string());
        }

        {
            let mut users = self.users.write().unwrap();

            if users.contains_key(&username) {
                return ServerResponse::SignupError(format!("User {username} already exists"));
            }

            let new_user = UserData::new(username.clone(), password.clone(), UserStatus::Offline);
            users.insert(username.clone(), new_user);
            status_to_notify = (username.clone(), UserStatus::Offline);
        }

        notify_status_update(&self.users, status_to_notify.0, status_to_notify.1);
        if let Err(e) = self.load_user_data_in_disk(username, password) {
            return ServerResponse::Error(format!("Error loading user data: {e}"));
        }
        ServerResponse::SignupOk
    }

    /// Logs out a user by switching its status to `Offline`.
    ///
    /// If the username does not exist, an error is returned.
    pub fn logout_user(&mut self, username: String) -> ServerResponse {
        let status_to_notify;
        {
            let mut users = self.users.write().unwrap();
            if let Some(user_data) = users.get_mut(&username) {
                user_data.update_status(UserStatus::Offline);
                let stream = match user_data.server_client_stream {
                    Some(ref stream) => stream,
                    None => return ServerResponse::Error("User not found".to_string()),
                };

                let mut stream = match stream.lock() {
                    Ok(stream) => stream,
                    Err(e) => return ServerResponse::Error(e.to_string()),
                };

                stream.conn.send_close_notify();
                stream.flush();
                stream.sock.shutdown(Shutdown::Both);

                self.users_connected
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                        if v == 0 { Some(0) } else { Some(v - 1) }
                    })
                    .ok();
                status_to_notify = (username.clone(), UserStatus::Offline);
            } else {
                return ServerResponse::LogoutError(format!("User {username} not found"));
            }
        }
        self.username = None;
        notify_status_update(&self.users, status_to_notify.0, status_to_notify.1);
        ServerResponse::LogoutOk
    }

    /// Attempts to establish a call between two users.
    ///
    /// Steps:
    /// 1. Retrieves `from_usr` and `to_usr` data.
    /// 2. Ensures both users are currently `Available`.
    /// 3. Sends a `CallIncoming` message to `to_usr`.
    /// 4. Waits for an answer using `get_answer_from_peer`.
    /// 5. Depending on the received response, either:
    ///    - accepts the call (`CallAccept`)
    ///    - rejects the call (`CallReject`)
    ///
    /// Errors are reported via `ServerResponse::Error`.
    pub fn call_request(
        &mut self,
        from_usr: String,
        to_usr: String,
        offer_sdp: SessionDescriptionProtocol,
    ) -> ServerResponse {
        let from_usr_data = match get_user_data(&from_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => return ServerResponse::Error(err_event),
        };

        let to_usr_data = match get_user_data(&to_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => return ServerResponse::Error(err_event),
        };

        if from_usr_data.status != UserStatus::Available {
            return ServerResponse::Error(ServerError::UserNotAvailable(from_usr).to_string());
        }

        if to_usr_data.status != UserStatus::Available {
            return ServerResponse::Error(ServerError::UserNotAvailable(to_usr).to_string());
        }

        let ans =
            match get_answer_from_peer(from_usr.clone(), to_usr.clone(), offer_sdp, &self.users) {
                Ok(answer) => answer,
                Err(err_event) => return ServerResponse::Error(err_event.to_string()),
            };

        match ans {
            ClientResponse::CallAccept { sdp_answer } => {
                self.call_accept(to_usr, from_usr, sdp_answer)
            }
            ClientResponse::CallReject => self.call_reject(to_usr, from_usr),
            _ => ServerResponse::Error("Invalid answer".to_string()),
        }
    }

    /// Handles a positive call answer.
    ///
    /// Preconditions:
    /// - both users must exist,
    /// - both must currently be `Available`.
    ///
    /// The function updates both users to an `Occupied` state, linking them
    /// to one another, and returns a `CallAccepted` server response with
    /// the caller's SDP answer.
    pub fn call_accept(
        &mut self,
        from_usr: String,
        to_usr: String,
        sdp_answer: SessionDescriptionProtocol,
    ) -> ServerResponse {
        let mut from_usr_data = match get_user_data(&from_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => return ServerResponse::Error(err_event),
        };

        let mut to_usr_data = match get_user_data(&to_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => return ServerResponse::Error(err_event),
        };

        if from_usr_data.status != UserStatus::Available {
            return ServerResponse::Error(ServerError::UserNotAvailable(from_usr).to_string());
        }

        if to_usr_data.status != UserStatus::Available {
            return ServerResponse::Error(ServerError::UserNotAvailable(to_usr).to_string());
        }

        from_usr_data.update_status(UserStatus::Occupied(to_usr_data.username.clone()));
        to_usr_data.update_status(UserStatus::Occupied(from_usr_data.username.clone()));

        {
            let mut users = self.users.write().unwrap();
            users.insert(to_usr_data.username.clone(), to_usr_data);
            users.insert(from_usr_data.username.clone(), from_usr_data);
        }

        notify_status_update(
            &self.users,
            from_usr.clone(),
            UserStatus::Occupied(to_usr.clone()),
        );
        notify_status_update(&self.users, to_usr, UserStatus::Occupied(from_usr));
        ServerResponse::CallAccepted { sdp_answer }
    }

    /// Handles a call rejection.
    ///
    /// Both users must be `Available`; otherwise an error is returned.
    ///
    /// No state change occurs — both users remain `Available`.
    pub fn call_reject(&mut self, from_usr: String, to_usr: String) -> ServerResponse {
        let from_usr_data = match get_user_data(&from_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => return ServerResponse::Error(err_event),
        };

        let to_usr_data = match get_user_data(&to_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => return ServerResponse::Error(err_event),
        };

        if from_usr_data.status != UserStatus::Available {
            return ServerResponse::Error(ServerError::UserNotAvailable(from_usr).to_string());
        }

        if to_usr_data.status != UserStatus::Available {
            return ServerResponse::Error(ServerError::UserNotAvailable(to_usr).to_string());
        }

        ServerResponse::CallRejected
    }

    /// Terminates an active call for the given user.
    ///
    /// The function checks:
    /// - that the user exists,
    /// - that their status is `Occupied`.
    ///
    /// If valid, the user's status is returned to `Available`.
    /// This function **does not** modify the status of the peer user.
    ///
    /// Returns `CallHangUpOk` on success, otherwise an appropriate error.
    pub fn call_hangup(&mut self, user: String) -> ServerResponse {
        {
            let mut users = self.users.write().unwrap();

            let data = match users.get_mut(&user) {
                Some(data) => data,
                None => return ServerResponse::CallHangUpError("User does not exist".to_string()),
            };

            match data.status {
                UserStatus::Occupied(_) => {}
                _ => return ServerResponse::CallHangUpError("Unexpected user status".to_string()),
            }

            data.update_status(UserStatus::Available);
        }
        notify_status_update(&self.users, user, UserStatus::Available);
        ServerResponse::CallHangUpOk
    }

    /// Returns the username lists for all three user states:
    /// `Available`, `Occupied`, and `Offline`.
    ///
    /// This is useful for a client wanting to know who is online and what
    /// their current status is.
    pub fn get_clients_for_user(&mut self, own_username: String) -> HashMap<String, UserStatus> {
        let users = {
            let usr = self.users.read().unwrap();
            usr.clone()
        };

        let mut users_map = HashMap::new();

        for (user, user_data) in users {
            if user == own_username {
                continue;
            }
            users_map.insert(user, user_data.status);
        }

        users_map
    }

    fn load_user_data_in_disk(
        &mut self,
        username: String,
        password: String,
    ) -> Result<(), ServerError> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.users_file_path)
            .map_err(|_| ServerError::OpenUserDataFileError)?;

        writeln!(file, "{username}:{password}").map_err(|_| ServerError::WriteUserDataFileError)?;

        Ok(())
    }

    pub fn make_user_offline(&mut self) -> Result<(), ServerError> {
        let username = match &self.username {
            Some(name) => name,
            None => return Ok(()),
        };

        {
            let mut users = self.users.write().unwrap();
            let user_data = users
                .get_mut(username)
                .ok_or(ServerError::UserNotAvailable(username.to_string()))?;
            user_data.update_status(UserStatus::Offline);
        }

        notify_status_update(&self.users, username.clone(), UserStatus::Offline);
        self.username = None;
        Ok(())
    }
}
/// Fetches a user record by name.
///
/// Unlike most internal helpers, this function returns a **cloned**
/// `UserData`, not a reference. This avoids borrow checker issues
/// without keeping the lock held.
///
/// # Errors
/// Returns `"User <name> not found"` if the user does not exist.
fn get_user_data(
    user_name: &String,
    users_data: &RwLock<HashMap<String, UserData>>,
) -> Result<UserData, String> {
    let users = users_data.read().map_err(|_| "Poisoned lock")?;
    match users.get(user_name) {
        Some(data) => Ok(data.clone()),
        None => Err(format!("User {user_name} not found")),
    }
}

/// Sends a `CallIncoming` notification to a user and waits for their reply.
///
/// This function writes a `ServerMessage::CallIncoming` to the target user's
/// `server_client_stream`, then blocks until the user responds with a
/// `ClientResponse`, such as:
///
/// - `CallAccept`
/// - `CallReject`
///
/// # Errors
/// Returns an `Error` if:
/// - the user does not exist,
/// - no stream is associated with the user,
/// - writing fails,
/// - the user disconnects,
/// - the user sends an invalid or unparsable response.
fn get_answer_from_peer(
    from_usr: String,
    to_usr: String,
    offer_sdp: SessionDescriptionProtocol,
    users: &RwLock<HashMap<String, UserData>>,
) -> Result<ClientResponse, ServerError> {
    let mut stream = get_stream_from_user(to_usr.clone(), users)?;

    send_message(
        &mut stream,
        &ServerMessage::CallIncoming {
            from: from_usr.clone(),
            offer_sdp,
        },
    );

    let mut buff = [0u8; 1024];

    let n = match stream.lock().unwrap().read(&mut buff) {
        Ok(0) => return Err(ServerError::MapError("Connection closed".to_string())),
        Ok(n) => n,
        Err(err) => return Err(ServerError::MapError(err.to_string())),
    };

    match ClientResponse::from_bytes(&buff[..n]) {
        Some(ans) => Ok(ans),
        None => Err(ServerError::MapError(
            "Failed to parse user answer from server".to_string(),
        )),
    }
}

fn validate_string(s: String) -> Result<(), ServerError> {
    let trimmed = s.trim();

    if trimmed.is_empty() {
        return Err(ServerError::InvalidFormat);
    };

    if trimmed.contains(' ') {
        return Err(ServerError::InvalidFormat);
    };

    Ok(())
}

/// Retrieves the server→client TCP stream for a given user.
///
/// This is used when the server needs to push a message to the client's
/// dedicated inbound channel (e.g., `CallIncoming`).
///
/// # Errors
/// Returns:
/// - `UserDoesNotExist` if the user does not exist or has no mapped stream,
/// - `PoisonedLock` if the internal `RwLock` was poisoned.
fn get_stream_from_user(
    username: String,
    users: &RwLock<HashMap<String, UserData>>,
) -> Result<Arc<Mutex<StreamOwned<ServerConnection, TcpStream>>>, ServerError> {
    let users = users.read().map_err(|_| ServerError::PoisonedLock)?;

    if let Some(user_data) = users.get(&username)
        && let Some(stream) = &user_data.server_client_stream
    {
        return Ok(stream.clone());
    }

    Err(ServerError::UserDoesNotExist(username))
}

/// Sends a serialized `ServerMessage` over a user's inbound stream.
///
/// This is used in situations where the server pushes notifications
/// to clients, such as:
/// - `CallIncoming`
/// - username requests
///
/// All errors are logged but not returned.
fn send_message(
    stream: &Arc<Mutex<StreamOwned<ServerConnection, TcpStream>>>,
    message: &ServerMessage,
) {
    let mut guard = match stream.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return;
        }
    };
    guard.write_all(&message.to_bytes());
}

fn notify_status_update(
    users: &Arc<RwLock<HashMap<String, UserData>>>,
    username: String,
    status: UserStatus,
) {
    let users = users.clone();
    thread::spawn(move || {
        let mut streams: Vec<Arc<Mutex<StreamOwned<ServerConnection, TcpStream>>>> = Vec::new();
        {
            let users_guard = users.read().unwrap();
            for (name, user_data) in users_guard.iter() {
                if *name == username {
                    continue;
                }
                if let Some(stream_arc) = &user_data.server_client_stream {
                    streams.push(stream_arc.clone());
                }
            }
        }
        let msg = ServerMessage::UserStatusUpdate(username.clone(), status);
        for stream in streams {
            send_message(&stream, &msg);
        }
    });
}

fn setup_logger() -> Logger {
    // We use unwrap here because panicking is acceptable in a test environment
    // We must pass a file path, even if we don't care about the output.
    Logger::new("test_server.log")
        .expect("Failed to create logger for test")
        .context("TestServer")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> OperatingServer {
        let users = setup_users();
        let users_connected = setup_users_connected(0);
        let logger = setup_logger();
        OperatingServer::new(
            users,
            users_connected,
            SocketAddr::new("127.0.0.1".parse().unwrap(), 8080),
            "test_users.txt".to_string(),
            logger,
        )
    }

    fn setup_with_users(map: Vec<(&str, &str, UserStatus)>) -> OperatingServer {
        let amount = map.len();
        let mut users = HashMap::new();
        for (name, passwd, status) in map {
            let data = UserData::new(name.to_string().clone(), passwd.to_string().clone(), status);
            users.insert(name.to_string(), data);
        }
        let users_connected = setup_users_connected(amount);

        let logger = setup_logger();

        OperatingServer::new(
            Arc::new(RwLock::new(users)),
            users_connected,
            SocketAddr::new("127.0.0.1".parse().unwrap(), 8080),
            "test_users.txt".to_string(),
            logger,
        )
    }

    fn setup_users() -> Arc<RwLock<HashMap<String, UserData>>> {
        Arc::new(RwLock::new(HashMap::new()))
    }

    fn setup_users_connected(n: usize) -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(n))
    }

    #[test]
    fn test_signup_ok() {
        let mut op_s = setup();
        let event = op_s.signup_user("alice".into(), "1234".into());
        assert!(matches!(event, ServerResponse::SignupOk));

        let map = op_s.users.read().unwrap();
        assert!(map.contains_key("alice"));
        assert_eq!(map["alice"].password, "1234");
    }

    #[test]
    fn test_signup_sets_status_offline_by_default() {
        let mut op_s = setup();
        let _ = op_s.signup_user("alice".into(), "1234".into());

        assert!(matches!(
            op_s.users
                .read()
                .unwrap()
                .get(&"alice".to_string())
                .unwrap()
                .status,
            UserStatus::Offline
        ));
    }

    #[test]
    fn test_signup_existing_user() {
        let mut op_s = setup();
        op_s.signup_user("bob".into(), "pass".into());

        let event = op_s.signup_user("bob".into(), "other".into());
        match event {
            ServerResponse::SignupError(msg) => assert!(msg.contains("already exists")),
            _ => panic!("Expected SignupError"),
        }
    }

    #[test]
    fn test_login_ok() {
        let mut op_s = setup();
        op_s.signup_user("maria".into(), "abcd".into());

        let event = op_s.login_user("maria".into(), "abcd".into());
        assert!(matches!(event, ServerResponse::LoginOk(_, _, _)));
    }

    #[test]
    fn test_login_changes_status_to_available() {
        let mut op_s = setup();
        op_s.signup_user("maria".into(), "abcd".into());

        let _ = op_s.login_user("maria".into(), "abcd".into());
        assert!(matches!(
            op_s.users
                .read()
                .unwrap()
                .get(&"maria".to_string())
                .unwrap()
                .status,
            UserStatus::Available
        ));
    }

    #[test]
    fn test_cannot_login_twice_with_same_user() {
        let mut op_s = setup();
        op_s.signup_user("alice".into(), "abcd".into());

        let event = op_s.login_user("alice".into(), "abcd".into());
        assert!(matches!(event, ServerResponse::LoginOk(_, _, _)));
        let event = op_s.login_user("alice".into(), "abcd".into());
        assert!(matches!(event, ServerResponse::LoginError(_)));
    }

    #[test]
    fn test_login_wrong_password() {
        let mut op_s = setup();
        op_s.signup_user("carl".into(), "mypw".into());

        let event = op_s.login_user("carl".into(), "WRONG".into());

        match event {
            ServerResponse::LoginError(msg) => assert!(msg.contains("Wrong password")),
            _ => panic!("Expected LoginError"),
        }
    }

    #[test]
    fn test_login_user_not_found() {
        let mut op_s = setup();

        let event = op_s.login_user("ghost".into(), "nopw".into());

        match event {
            ServerResponse::LoginError(msg) => assert!(msg.contains("not found")),
            _ => panic!("Expected LoginError"),
        }
    }

    #[test]
    fn test_login_adds_one_to_users_connected() {
        let mut op_s = setup();

        op_s.signup_user("alice".into(), "1234".into());
        let _ = op_s.login_user("alice".into(), "1234".into());
        let usr = op_s.users_connected.load(Ordering::SeqCst);
        assert_eq!(usr, 1);
    }

    #[test]
    fn logout_user_success() {
        let mut op_s = setup_with_users(vec![("alice", "1233", UserStatus::Available)]);

        let result = op_s.logout_user("alice".to_string());

        assert!(matches!(result, ServerResponse::LogoutOk));

        let users = op_s.users.read().unwrap();
        let user = users.get("alice").unwrap();
        assert!(matches!(user.status, UserStatus::Offline));
    }

    #[test]
    fn logout_user_not_found() {
        let mut op_s = setup();

        let result = op_s.logout_user("ghost".to_string());

        assert!(matches!(result, ServerResponse::LogoutError(msg) if msg.contains("ghost")));
    }

    #[test]
    fn logout_user_does_not_affect_others() {
        let mut op_s = setup_with_users(vec![
            ("bob", "xxx", UserStatus::Available),
            ("eve", "yyy", UserStatus::Occupied("someone".to_string())),
        ]);

        let _ = op_s.logout_user("bob".to_string());

        let users = op_s.users.read().unwrap();
        assert!(matches!(users["bob"].status, UserStatus::Offline));
        assert!(matches!(users["eve"].status, UserStatus::Occupied(_)));
    }

    #[test]
    fn test_get_user_data_ok() {
        let op_s = setup_with_users(vec![("alice", "1234", UserStatus::Available)]);

        let res = get_user_data(&"alice".to_string(), &op_s.users);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().username, "alice");
    }

    #[test]
    fn test_get_user_data_not_found() {
        let op_s = setup();
        let res = get_user_data(&"ghost".to_string(), &op_s.users);
        assert!(res.is_err());
    }

    #[test]
    fn test_call_hangup_ok() {
        let mut op_s = setup_with_users(vec![("alice", "xxx", UserStatus::Occupied("bob".into()))]);

        let result = op_s.call_hangup("alice".to_string());
        assert!(matches!(result, ServerResponse::CallHangUpOk));

        let users = op_s.users.read().unwrap();
        assert!(matches!(users["alice"].status, UserStatus::Available));
    }

    #[test]
    fn test_call_hangup_unexpected_status() {
        let mut op_s = setup_with_users(vec![("alice", "xxx", UserStatus::Available)]);

        let result = op_s.call_hangup("alice".into());

        assert!(matches!(result,
        ServerResponse::CallHangUpError(msg) if msg.contains("Unexpected")));
    }

    #[test]
    fn test_call_accept_success() {
        let mut op_s = setup_with_users(vec![
            ("bob", "123", UserStatus::Available),
            ("alice", "abc", UserStatus::Available),
        ]);

        let _ = op_s.call_accept(
            "alice".into(),
            "bob".into(),
            SessionDescriptionProtocol::default(),
        );

        let map = op_s.users.read().unwrap();

        let alice = map.get("alice").unwrap();
        let bob = map.get("bob").unwrap();

        match &alice.status {
            UserStatus::Occupied(who) => assert_eq!(who, "bob"),
            _ => panic!("Alice should be occupied by Bob"),
        }

        match &bob.status {
            UserStatus::Occupied(who) => assert_eq!(who, "alice"),
            _ => panic!("Bob should be occupied by Alice"),
        }
    }

    #[test]
    fn test_call_accept_from_user_not_available() {
        let mut op_s = setup_with_users(vec![
            ("alice", "123", UserStatus::Occupied("x".into())),
            ("bob", "abc", UserStatus::Available),
        ]);

        let resp = op_s.call_accept(
            "alice".into(),
            "bob".into(),
            SessionDescriptionProtocol::default(),
        );

        match resp {
            ServerResponse::Error(msg) => {
                assert!(msg.contains("user not available"));
                assert!(msg.contains("alice"));
            }
            _ => panic!("Expected Error(UserNotAvailable)"),
        }
    }

    #[test]
    fn test_call_accept_to_user_not_available() {
        let mut op_s = setup_with_users(vec![
            ("bob", "123", UserStatus::Occupied("x".into())),
            ("alice", "abc", UserStatus::Available),
        ]);

        let resp = op_s.call_accept(
            "alice".into(),
            "bob".into(),
            SessionDescriptionProtocol::default(),
        );

        match resp {
            ServerResponse::Error(msg) => {
                assert!(msg.contains("user not available"));
                assert!(msg.contains("bob"));
            }
            _ => panic!("Expected Error(UserNotAvailable)"),
        }
    }

    #[test]
    fn test_call_accept_from_user_not_found() {
        let mut op_s = setup_with_users(vec![("bob", "123", UserStatus::Available)]);

        let resp = op_s.call_accept(
            "alice".into(),
            "bob".into(),
            SessionDescriptionProtocol::default(),
        );

        match resp {
            ServerResponse::Error(msg) => {
                assert!(msg.contains("UserNotFound") || msg.contains("not found"));
            }
            _ => panic!("Expected Error(UserNotFound)"),
        }
    }

    #[test]
    fn test_call_accept_to_user_not_found() {
        let mut op_s = setup_with_users(vec![("bob", "123", UserStatus::Available)]);

        let resp = op_s.call_accept(
            "alice".into(),
            "bob".into(),
            SessionDescriptionProtocol::default(),
        );

        match resp {
            ServerResponse::Error(msg) => {
                assert!(msg.contains("UserNotFound") || msg.contains("not found"));
            }
            _ => panic!("Expected Error(UserNotFound)"),
        }
    }

    #[test]
    fn test_call_reject_ok() {
        let mut op_s = setup_with_users(vec![
            ("bob", "abc", UserStatus::Available),
            ("alice", "123", UserStatus::Available),
        ]);

        {
            let mut u = op_s.users.write().unwrap();
            u.get_mut("alice")
                .unwrap()
                .update_status(UserStatus::Available);
            u.get_mut("bob")
                .unwrap()
                .update_status(UserStatus::Available);
        }

        let _ = op_s.call_reject("alice".into(), "bob".into());

        {
            let u = op_s.users.write().unwrap();
            let alice = u.get("alice").unwrap();
            let bob = u.get("bob").unwrap();

            assert_eq!(alice.status, UserStatus::Available);
            assert_eq!(bob.status, UserStatus::Available);
        };
    }

    #[test]
    fn test_call_reject_user_not_available() {
        let mut op_s = setup_with_users(vec![
            ("bob", "abc", UserStatus::Available),
            ("alice", "123", UserStatus::Available),
        ]);

        {
            let mut u = op_s.users.write().unwrap();
            u.get_mut("alice")
                .unwrap()
                .update_status(UserStatus::Occupied("bob".into()));
            u.get_mut("bob")
                .unwrap()
                .update_status(UserStatus::Available);
        }

        let resp = op_s.call_reject("alice".into(), "bob".into());

        match resp {
            ServerResponse::Error(msg) => {
                assert!(msg.contains("alice"));
            }
            _ => panic!("Expected Error because alice is not available"),
        }
    }
}
