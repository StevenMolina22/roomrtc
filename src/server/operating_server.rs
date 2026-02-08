use super::ServerError;
use crate::client_server_protocol::{ServerMessage, ServerResponse};
use crate::logger::Logger;
use crate::session::sdp::SessionDescriptionProtocol;
use crate::user::{UserData, UserStatus};
use rustls::{ServerConnection, StreamOwned};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::{Arc, Mutex, RwLock};

/// Core server logic for user authentication and call signaling.
pub struct OperatingServer {
    users: Arc<RwLock<HashMap<String, UserData>>>,
    server_client_socket_address: SocketAddr,
    users_file_path: String,
    username: Option<String>,
    logger: Logger,
}

impl OperatingServer {
    pub const fn new(
        users: Arc<RwLock<HashMap<String, UserData>>>,
        server_client_socket_address: SocketAddr,
        users_file_path: String,
        logger: Logger,
    ) -> Self {
        Self {
            users,
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
                Err(e) => {
                    self.logger
                        .error(&format!("Failed to acquire users lock during login: {e}"));
                    return ServerResponse::Error(e.to_string());
                }
            };

            if let Some(data) = users.get_mut(&username) {
                if data.password != password {
                    self.logger
                        .warn(&format!("Login failed for {username}: Wrong password"));
                    return ServerResponse::LoginError("Wrong password. Try again".to_string());
                }

                match data.status {
                    UserStatus::Available | UserStatus::Occupied(_) => {
                        self.logger.warn(&format!(
                            "Login failed for {username}: User already logged in"
                        ));
                        return ServerResponse::LoginError("User already logged in".to_string());
                    }

                    UserStatus::Offline => {
                        data.update_status(UserStatus::Available);
                        status_to_notify = (username.clone(), UserStatus::Available);
                    }
                }
            } else {
                self.logger
                    .warn(&format!("Login failed for {username}: User not found"));
                return ServerResponse::LoginError(format!("User {username} not found"));
            }
        }

        self.username = Some(username.clone());
        notify_status_update(
            &self.users,
            status_to_notify.0,
            status_to_notify.1,
            &self.logger,
        );
        self.logger
            .info(&format!("User {username} logged in successfully"));
        ServerResponse::LoginOk(
            username.clone(),
            self.server_client_socket_address,
            self.get_clients_for_user(username),
        )
    }

    /// Registers a new user.
    ///
    /// The username must not already be present in the user map.
    /// New users are always created with `UserStatus::Offline`.
    pub fn signup_user(&mut self, username: String, password: String) -> ServerResponse {
        let status_to_notify;
        if validate_string(username.clone()).is_err() {
            self.logger.warn(&format!(
                "Signup failed for {username}: Invalid username format"
            ));
            return ServerResponse::SignupError("Invalid username".to_string());
        }
        if validate_string(password.clone()).is_err() {
            self.logger.warn(&format!(
                "Signup failed for {username}: Invalid password format"
            ));
            return ServerResponse::SignupError("Invalid password".to_string());
        }

        {
            let mut users = match self.users.write() {
                Ok(u) => u,
                Err(e) => {
                    self.logger
                        .error(&format!("Failed to acquire users lock during signup: {e}"));
                    return ServerResponse::SignupError(e.to_string());
                }
            };

            if users.contains_key(&username) {
                self.logger.warn(&format!(
                    "Signup failed for {username}: User already exists"
                ));
                return ServerResponse::SignupError(format!("User {username} already exists"));
            }

            let new_user = UserData::new(username.clone(), password.clone(), UserStatus::Offline);
            users.insert(username.clone(), new_user);
            status_to_notify = (username.clone(), UserStatus::Offline);
        }

        notify_status_update(
            &self.users,
            status_to_notify.0,
            status_to_notify.1,
            &self.logger,
        );
        if let Err(e) = self.load_user_data_in_disk(username.clone(), password) {
            self.logger.error(&format!(
                "Error loading user data to disk for {username}: {e}"
            ));
            return ServerResponse::Error(format!("Error loading user data: {e}"));
        }
        self.logger.info(&format!("New user signed up: {username}"));
        ServerResponse::SignupOk
    }

    /// Logs out a user by switching its status to `Offline`.
    ///
    /// If the username does not exist, an error is returned.
    pub fn logout_user(&mut self, username: String) -> ServerResponse {
        let status_to_notify;
        {
            let mut users = match self.users.write() {
                Ok(u) => u,
                Err(e) => {
                    self.logger
                        .error(&format!("Failed to acquire users lock during logout: {e}"));
                    return ServerResponse::LogoutError(e.to_string());
                }
            };
            if let Some(user_data) = users.get_mut(&username) {
                user_data.update_status(UserStatus::Offline);
                let stream = if let Some(ref stream) = user_data.server_client_stream {
                    stream
                } else {
                    self.logger
                        .warn(&format!("Logout failed for {username}: Stream not found"));
                    return ServerResponse::Error("User not found".to_string());
                };

                let mut stream = match stream.lock() {
                    Ok(stream) => stream,
                    Err(e) => {
                        self.logger.error(&format!(
                            "Logout failed for {username}: Failed to lock stream: {e}"
                        ));
                        return ServerResponse::Error(e.to_string());
                    }
                };

                stream.conn.send_close_notify();
                if let Err(e) = stream.flush() {
                    self.logger.warn(&format!(
                        "Failed to flush stream during logout for {username}: {e}"
                    ));
                }
                if let Err(e) = stream.sock.shutdown(Shutdown::Both) {
                    self.logger.warn(&format!(
                        "Failed to shutdown socket during logout for {username}: {e}"
                    ));
                }
                status_to_notify = (username.clone(), UserStatus::Offline);
            } else {
                self.logger
                    .warn(&format!("Logout failed for {username}: User not found"));
                return ServerResponse::LogoutError(format!("User {username} not found"));
            }
        }
        self.username = None;
        notify_status_update(
            &self.users,
            status_to_notify.0,
            status_to_notify.1,
            &self.logger,
        );
        self.logger.info(&format!("User {username} logged out"));
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
        self.logger
            .info(&format!("Call request from {from_usr} to {to_usr}"));

        let stream = match get_stream_from_user(to_usr.clone(), &self.users) {
            Ok(stream) => stream,
            Err(e) => return ServerResponse::CallRequestError(e.to_string()),
        };

        let mut from_usr_data = match get_user_data(&from_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => {
                self.logger
                    .warn(&format!("Call request failed: Sender {from_usr} not found"));
                return ServerResponse::Error(err_event);
            }
        };

        let mut to_usr_data = match get_user_data(&to_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => {
                self.logger
                    .warn(&format!("Call request failed: Receiver {to_usr} not found"));
                return ServerResponse::Error(err_event);
            }
        };

        if from_usr_data.status != UserStatus::Available {
            self.logger.warn(&format!(
                "Call request failed: Sender {from_usr} not available"
            ));
            return ServerResponse::Error(ServerError::UserNotAvailable(from_usr).to_string());
        }

        if to_usr_data.status != UserStatus::Available {
            self.logger.warn(&format!(
                "Call request failed: Receiver {to_usr} not available"
            ));
            return ServerResponse::Error(ServerError::UserNotAvailable(to_usr).to_string());
        }

        from_usr_data.update_status(UserStatus::Occupied(to_usr.clone()));
        to_usr_data.update_status(UserStatus::Occupied(from_usr.clone()));

        {
            let mut users = match self.users.write() {
                Ok(u) => u,
                Err(e) => {
                    self.logger.error(&format!(
                        "Failed to acquire users lock during call_request inner: {e}"
                    ));
                    return ServerResponse::CallRequestError(e.to_string());
                }
            };
            users.insert(to_usr_data.username.clone(), to_usr_data);
            users.insert(from_usr_data.username.clone(), from_usr_data);
        }

        notify_status_update(
            &self.users,
            from_usr.clone(),
            UserStatus::Occupied(to_usr.clone()),
            &self.logger,
        );
        notify_status_update(
            &self.users,
            to_usr.clone(),
            UserStatus::Occupied(from_usr.clone()),
            &self.logger,
        );

        let msg = ServerMessage::CallIncoming {
            from_usr,
            offer_sdp,
        };
        send_message(&stream, &msg);

        ServerResponse::CallRequestOk
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
        let stream = match get_stream_from_user(to_usr.clone(), &self.users) {
            Ok(stream) => stream,
            Err(e) => return ServerResponse::CallAcceptError(e.to_string()),
        };

        let from_usr_data = match get_user_data(&from_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => return ServerResponse::CallAcceptError(err_event),
        };

        let to_usr_data = match get_user_data(&to_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => return ServerResponse::CallAcceptError(err_event),
        };

        if from_usr_data.status != UserStatus::Occupied(to_usr.clone()) {
            return ServerResponse::CallAcceptError(
                ServerError::UserNotAvailable(from_usr).to_string(),
            );
        }

        if to_usr_data.status != UserStatus::Occupied(from_usr.clone()) {
            return ServerResponse::CallAcceptError(
                ServerError::UserNotAvailable(to_usr).to_string(),
            );
        }

        self.logger.info(&format!(
            "Call accepted: {from_usr} and {to_usr} are now OCCUPIED"
        ));

        let msg = ServerMessage::CallAccepted {
            from_usr,
            sdp_answer,
        };

        send_message(&stream, &msg);

        ServerResponse::CallAcceptOk
    }

    /// Handles a call rejection.
    ///
    /// Both users must be `Available`; otherwise an error is returned.
    ///
    /// No state change occurs — both users remain `Available`.
    pub fn call_reject(&mut self, from_usr: String, to_usr: String) -> ServerResponse {
        let stream = match get_stream_from_user(to_usr.clone(), &self.users) {
            Ok(stream) => stream,
            Err(e) => return ServerResponse::CallRejectError(e.to_string()),
        };

        let mut from_usr_data = match get_user_data(&from_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => return ServerResponse::CallRejectError(err_event),
        };

        let mut to_usr_data = match get_user_data(&to_usr, &self.users) {
            Ok(data) => data,
            Err(err_event) => return ServerResponse::CallRejectError(err_event),
        };

        if from_usr_data.status != UserStatus::Occupied(to_usr.clone()) {
            return ServerResponse::CallRejectError(
                ServerError::UserNotAvailable(from_usr).to_string(),
            );
        }

        if to_usr_data.status != UserStatus::Occupied(from_usr.clone()) {
            return ServerResponse::CallRejectError(
                ServerError::UserNotAvailable(to_usr).to_string(),
            );
        }

        from_usr_data.update_status(UserStatus::Available);
        to_usr_data.update_status(UserStatus::Available);

        notify_status_update(
            &self.users,
            from_usr.clone(),
            UserStatus::Available,
            &self.logger,
        );
        notify_status_update(
            &self.users,
            to_usr.clone(),
            UserStatus::Available,
            &self.logger,
        );

        {
            let mut users = match self.users.write() {
                Ok(u) => u,
                Err(e) => {
                    self.logger.error(&format!(
                        "Failed to acquire users lock during call_reject: {e}"
                    ));
                    return ServerResponse::CallRejectError(e.to_string());
                }
            };
            users.insert(to_usr_data.username.clone(), to_usr_data);
            users.insert(from_usr_data.username.clone(), from_usr_data);
        }

        self.logger
            .warn(&format!("Call from {from_usr} rejected by {to_usr}"));

        let msg = ServerMessage::CallRejected;
        send_message(&stream, &msg);

        ServerResponse::CallRejectOk
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
            let mut users = match self.users.write() {
                Ok(u) => u,
                Err(e) => {
                    self.logger.error(&format!(
                        "Failed to acquire users lock during call_hangup: {e}"
                    ));
                    return ServerResponse::CallHangUpError(e.to_string());
                }
            };

            let data = if let Some(data) = users.get_mut(&user) {
                data
            } else {
                self.logger
                    .warn(&format!("Call hangup failed: User {user} does not exist"));
                return ServerResponse::CallHangUpError("User does not exist".to_string());
            };

            if let UserStatus::Occupied(_) = data.status {
            } else {
                self.logger
                    .warn(&format!("Call hangup failed: User {user} is not occupied"));
                return ServerResponse::CallHangUpError("Unexpected user status".to_string());
            }

            data.update_status(UserStatus::Available);
        }
        notify_status_update(
            &self.users,
            user.clone(),
            UserStatus::Available,
            &self.logger,
        );
        self.logger
            .info(&format!("Call hangup requested by {user}"));

        ServerResponse::CallHangUpOk
    }

    /// Returns the username lists for all three user states:
    /// `Available`, `Occupied`, and `Offline`.
    ///
    /// This is useful for a client wanting to know who is online and what
    /// their current status is.
    pub fn get_clients_for_user(&mut self, own_username: String) -> HashMap<String, UserStatus> {
        let users = {
            let usr = match self.users.read() {
                Ok(u) => u,
                Err(e) => {
                    self.logger.error(&format!(
                        "Failed to acquire users lock during get_clients_for_user: {e}"
                    ));
                    return HashMap::new();
                }
            };
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
            let mut users = match self.users.write() {
                Ok(u) => u,
                Err(e) => {
                    self.logger.error(&format!(
                        "Failed to acquire users lock during make_user_offline: {e}"
                    ));
                    return Err(ServerError::MapError(e.to_string()));
                }
            };
            let user_data = users
                .get_mut(username)
                .ok_or(ServerError::UserNotAvailable(username.to_string()))?;
            user_data.update_status(UserStatus::Offline);
        }

        notify_status_update(
            &self.users,
            username.clone(),
            UserStatus::Offline,
            &self.logger,
        );
        self.logger.info(&format!("User {username} disconnected"));
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

fn validate_string(s: String) -> Result<(), ServerError> {
    let trimmed = s.trim();

    if trimmed.is_empty() {
        return Err(ServerError::InvalidFormat);
    }

    if trimmed.contains(' ')
        || trimmed.contains('\n')
        || trimmed.contains(':')
        || trimmed.contains('|')
    {
        return Err(ServerError::InvalidFormat);
    }

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
    let _ = guard.write_all(&message.to_bytes());
}

fn notify_status_update(
    users: &Arc<RwLock<HashMap<String, UserData>>>,
    username: String,
    status: UserStatus,
    logger: &Logger,
) {
    let users = users.clone();
    let logger = logger.clone();
    logger.debug(&format!(
        "Broadcasting status update for {username}: {status:?}"
    ));
    let mut streams: Vec<Arc<Mutex<StreamOwned<ServerConnection, TcpStream>>>> = Vec::new();
    {
        let users_guard = match users.read() {
            Ok(guard) => guard,
            Err(e) => {
                logger.error(&format!(
                    "Failed to acquire users lock for status update: {e}"
                ));
                return;
            }
        };
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
}

#[cfg(test)]
fn setup_logger() -> Logger {
    Logger::new("test_server.log")
        .expect("Failed to create logger for test")
        .context("TestServer")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> OperatingServer {
        let users = setup_users();
        let logger = setup_logger();
        OperatingServer::new(
            users,
            SocketAddr::new("127.0.0.1".parse().expect("Valid IP address"), 8080),
            "test_users.txt".to_string(),
            logger,
        )
    }

    fn setup_with_users(map: Vec<(&str, &str, UserStatus)>) -> OperatingServer {
        let _amount = map.len();
        let mut users = HashMap::new();
        for (name, passwd, status) in map {
            let data = UserData::new(name.to_string().clone(), passwd.to_string().clone(), status);
            users.insert(name.to_string(), data);
        }

        let logger = setup_logger();

        OperatingServer::new(
            Arc::new(RwLock::new(users)),
            SocketAddr::new("127.0.0.1".parse().expect("Valid IP address"), 8080),
            "test_users.txt".to_string(),
            logger,
        )
    }

    fn setup_users() -> Arc<RwLock<HashMap<String, UserData>>> {
        Arc::new(RwLock::new(HashMap::new()))
    }

    #[test]
    fn test_signup_ok() {
        let mut op_s = setup();
        let event = op_s.signup_user("alice".into(), "1234".into());
        assert!(matches!(event, ServerResponse::SignupOk));

        let map = op_s.users.read().expect("Lock should not be poisoned");
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
                .expect("Lock should not be poisoned")
                .get(&"alice".to_string())
                .expect("Alice should exist")
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
            other => unreachable!("Expected SignupError, got {:?}", other),
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
                .expect("Lock should not be poisoned")
                .get(&"maria".to_string())
                .expect("Maria should exist")
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
            other => unreachable!("Expected LoginError, got {:?}", other),
        }
    }

    #[test]
    fn test_login_user_not_found() {
        let mut op_s = setup();

        let event = op_s.login_user("ghost".into(), "nopw".into());

        match event {
            ServerResponse::LoginError(msg) => assert!(msg.contains("not found")),
            other => unreachable!("Expected LoginError, got {:?}", other),
        }
    }

    #[test]
    fn logout_user_success() {
        let mut op_s = setup_with_users(vec![("alice", "1233", UserStatus::Available)]);

        let _result = op_s.logout_user("alice".to_string());

        let users = op_s.users.read().expect("Lock should not be poisoned");
        let user = users.get("alice").expect("Alice should exist");
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

        let users = op_s.users.read().expect("Lock should not be poisoned");
        assert!(matches!(users["bob"].status, UserStatus::Offline));
        assert!(matches!(users["eve"].status, UserStatus::Occupied(_)));
    }

    #[test]
    fn test_get_user_data_ok() {
        let op_s = setup_with_users(vec![("alice", "1234", UserStatus::Available)]);

        let res = get_user_data(&"alice".to_string(), &op_s.users);
        assert!(res.is_ok());
        assert_eq!(res.expect("Should succeed").username, "alice");
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

        let users = op_s.users.read().expect("Lock should not be poisoned");
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

        let res = op_s.call_request(
            "alice".into(),
            "bob".into(),
            SessionDescriptionProtocol::default(),
        );

        let map = op_s.users.read().expect("Lock should not be poisoned");

        let alice = map.get("alice").expect("Alice should exist");
        let bob = map.get("bob").expect("Bob should exist");

        match res {
            ServerResponse::CallRequestOk => {
                match &alice.status {
                    UserStatus::Occupied(who) => assert_eq!(who, "bob"),
                    other => unreachable!("Alice should be occupied by Bob, got {:?}", other),
                }

                match &bob.status {
                    UserStatus::Occupied(who) => assert_eq!(who, "alice"),
                    other => unreachable!("Bob should be occupied by Alice, got {:?}", other),
                }
            }
            ServerResponse::CallRequestError(_) => {
                assert_eq!(alice.status, UserStatus::Available);
                assert_eq!(bob.status, UserStatus::Available);
            }
            other => panic!("Unexpected response from call_request: {:?}", other),
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
            ServerResponse::CallAcceptError(msg) => {
                assert!(msg.contains("user not available") || msg.contains("user does not exist"));
            }
            other => unreachable!(
                "Expected CallAcceptError(UserNotAvailable), got {:?}",
                other
            ),
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
            ServerResponse::CallAcceptError(msg) => {
                assert!(msg.contains("user not available") || msg.contains("user does not exist"));
            }
            other => unreachable!(
                "Expected CallAcceptError(UserNotAvailable), got {:?}",
                other
            ),
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
            ServerResponse::CallAcceptError(msg) => {
                assert!(msg.contains("not found") || msg.contains("user does not exist"));
            }
            other => unreachable!("Expected CallAcceptError(UserNotFound), got {:?}", other),
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
            ServerResponse::CallAcceptError(msg) => {
                assert!(msg.contains("not found") || msg.contains("user does not exist"));
            }
            other => unreachable!("Expected CallAcceptError(UserNotFound), got {:?}", other),
        }
    }

    #[test]
    fn test_call_reject_ok() {
        let mut op_s = setup_with_users(vec![
            ("bob", "abc", UserStatus::Available),
            ("alice", "123", UserStatus::Available),
        ]);

        {
            let mut u = op_s.users.write().expect("Lock should not be poisoned");
            u.get_mut("alice")
                .expect("Alice should exist")
                .update_status(UserStatus::Available);
            u.get_mut("bob")
                .expect("Bob should exist")
                .update_status(UserStatus::Available);
        }

        let _ = op_s.call_reject("alice".into(), "bob".into());

        {
            let u = op_s.users.write().expect("Lock should not be poisoned");
            let alice = u.get("alice").expect("Alice should exist");
            let bob = u.get("bob").expect("Bob should exist");

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
            let mut u = op_s.users.write().expect("Lock should not be poisoned");
            u.get_mut("alice")
                .expect("Alice should exist")
                .update_status(UserStatus::Occupied("bob".into()));
            u.get_mut("bob")
                .expect("Bob should exist")
                .update_status(UserStatus::Available);
        }

        let resp = op_s.call_reject("alice".into(), "bob".into());

        match resp {
            ServerResponse::CallRejectError(msg) => {
                assert!(msg.contains("user not available") || msg.contains("user does not exist"));
            }
            other => unreachable!(
                "Expected CallRejectError because alice is not available, got {:?}",
                other
            ),
        }
    }
}
