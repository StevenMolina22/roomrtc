use super::{ServerError as Error, operating_server::OperatingServer};
use crate::client_server_protocol::{ClientMessage, ServerResponse};
use crate::config::Config;
use crate::logger::Logger;
use crate::user::UserData;
use rustls::{ServerConnection, StreamOwned};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

/// Manages a single client connection and delegates to `OperatingServer`.
pub struct UserHandler {
    op_server: OperatingServer,
    logger: Logger,
}
/// Handles the lifecycle of a connected client.
///
/// This loop:
/// - waits for raw bytes on the client's TCP connection,
/// - attempts to decode them into a `ClientMessage`,
/// - forwards the message to `handle_client_message`,
/// - sends the resulting `ServerResponse` back to the client.
///
/// The connection terminates if:
/// - the client disconnects,
/// - any I/O error occurs,
/// - the received bytes cannot be parsed into a valid message.
impl UserHandler {
    pub fn new(
        users: Arc<RwLock<HashMap<String, UserData>>>,
        users_connected: Arc<AtomicUsize>,
        config: Arc<Config>,
        server_client_socket_addr: SocketAddr,
        max_users: usize,
        logger: Logger,
    ) -> Self {
        Self {
            op_server: OperatingServer::new(
                users,
                users_connected,
                server_client_socket_addr,
                config.server.users_file.clone(),
                max_users,
                logger.context("OperatingServer"),
            ),
            logger,
        }
    }
    /// Handles the lifecycle of a connected client.
    ///
    /// This loop:
    /// - waits for raw bytes on the client's TCP connection,
    /// - attempts to decode them into a `ClientMessage`,
    /// - forwards the message to `handle_client_message`,
    /// - sends the resulting `ServerResponse` back to the client.
    ///
    /// The connection terminates if:
    /// - the client disconnects,
    /// - any I/O error occurs,
    /// - the received bytes cannot be parsed into a valid message.
    pub fn handle_client(
        &mut self,
        mut stream: StreamOwned<ServerConnection, TcpStream>,
        on: Arc<AtomicBool>,
    ) -> Result<(), Error> {
        let mut buff = [0; 1024];
        loop {
            if !on.load(Ordering::SeqCst) {
                return Err(Error::ServerOff);
            }
            let client_event = match stream.read(&mut buff) {
                Ok(0) => {
                    self.logger.info("Client disconnected (EOF)");
                    self.op_server.make_user_offline()?;
                    return Ok(());
                }
                Ok(n) => ClientMessage::from_bytes(&buff[0..n]),
                Err(e) => {
                    self.logger.error(&format!("Connection error: {e}"));
                    self.op_server.make_user_offline()?;
                    return Err(Error::ConnectionError(e.to_string()));
                }
            };

            let sv_response = match client_event {
                Some(event) => self.handle_client_message(event),
                None => ServerResponse::BadMessage,
            };

            send_response(&mut stream, sv_response);
        }
    }

    /// Dispatches a parsed `ClientMessage` to the appropriate server handler.
    ///
    /// Each message variant triggers a different operation:
    /// - `Login` → `login_user`
    /// - `Signup` → `signup_user`
    /// - `Logout` → `logout_user`
    /// - `CallRequest` → `call_request`
    /// - `CallHangup` → `call_hungup`
    /// - `SeeStatusClients` → `see_status_clients`
    ///
    /// Returns a `ServerResponse` that is sent back to the client.
    fn handle_client_message(&mut self, event: ClientMessage) -> ServerResponse {
        match event {
            ClientMessage::LogIn { 
                username, 
                password 
            } => self.op_server.login_user(username, password),

            ClientMessage::SignUp { 
                username, 
                password 
            } => self.op_server.signup_user(username, password),
            

            ClientMessage::LogOut { 
                token 
            } => self.op_server.logout_user(token),

            ClientMessage::CallRequest {
                token,
                offer_sdp,
                to,
            } => self.op_server.call_request(token, to, offer_sdp),
            
            ClientMessage::CallAccept { 
                from_usr, 
                to_usr, 
                sdp_answer 
            } => self.op_server.call_accept(from_usr, to_usr, sdp_answer),
            
            ClientMessage::CallReject {
                from_usr,
                to_usr,
            } => self.op_server.call_reject(from_usr, to_usr),

            ClientMessage::CallHangup { token } => self.op_server.call_hangup(token),
        }
    }
}

/// Sends a serialized `ServerResponse` over the TCP stream.
///
/// Any I/O error is logged but not returned to the caller.
fn send_response(stream: &mut StreamOwned<ServerConnection, TcpStream>, response: ServerResponse) {
    let _ = stream.write_all(&response.to_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_server_protocol::ClientMessage;
    use crate::session::sdp::SessionDescriptionProtocol;
    use crate::user::UserStatus;

    fn setup_handler() -> (UserHandler, Arc<RwLock<HashMap<String, UserData>>>) {
        let users = Arc::new(RwLock::new(HashMap::new()));
        let users_ref = users.clone();
        
        let users_connected = Arc::new(AtomicUsize::new(0));
        let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

        let logger = match Logger::new("/dev/null") {
            Ok(l) => l,
            Err(_) => match Logger::new("test_fallback.log") {
                Ok(l2) => l2,
                Err(e) => panic!("Failed to create logger for tests: {e}"),
            },
        };

        let op_server = OperatingServer::new(
            users,
            users_connected,
            addr,
            "test_handler_users.txt".into(),
            10,
            logger.clone(),
        );

        let handler = UserHandler {
            op_server,
            logger,
        };

        (handler, users_ref)
    }

    #[test]
    fn test_dispatch_login() {
        let (mut handler, _) = setup_handler();
        
        handler.op_server.signup_user("tester".into(), "pass".into());

        let msg = ClientMessage::LogIn { 
            username: "tester".into(), 
            password: "pass".into() 
        };

        let response = handler.handle_client_message(msg);
        
        match response {
            ServerResponse::LoginOk(u, _, _) => assert_eq!(u, "tester"),
            _ => assert!(false, "El mensaje LogIn no devolvió LoginOk"),
        }
    }

    #[test]
    fn test_dispatch_signup() {
        let (mut handler, _) = setup_handler();
        
        let msg = ClientMessage::SignUp { 
            username: "newuser".into(), 
            password: "123".into() 
        };

        let response = handler.handle_client_message(msg);
        assert!(matches!(response, ServerResponse::SignupOk));
    }

    #[test]
    fn test_dispatch_logout() {
        let (mut handler, users_ref) = setup_handler();
        
        handler.op_server.signup_user("leaver".into(), "pass".into());
        handler.op_server.login_user("leaver".into(), "pass".into());

        let msg = ClientMessage::LogOut { token: "leaver".into() };
        let response = handler.handle_client_message(msg);
        
        assert!(matches!(response, ServerResponse::Error(_))); 
        
        if let Ok(users) = users_ref.read()
            && let Some(user) = users.get("leaver") {
                assert_eq!(user.status, UserStatus::Offline);
            }
        
    }

    #[test]
    fn test_dispatch_call_hangup() {
        let (mut handler, users_ref) = setup_handler();
        
        // Creamos usuario
        handler.op_server.signup_user("busy".into(), "pass".into());
        
        if let Ok(mut u) = users_ref.write()
            && let Some(data) = u.get_mut("busy") {
                data.status = UserStatus::Occupied("other".into());
            }
        

        let msg = ClientMessage::CallHangup { token: "busy".into() };
        let response = handler.handle_client_message(msg);
        
        assert!(matches!(response, ServerResponse::CallHangUpOk));
    }

    #[test]
    fn test_dispatch_call_request_validation() {
        let (mut handler, _) = setup_handler();
        
        let msg = ClientMessage::CallRequest { 
            token: "origin".into(), 
            to: "dest".into(), 
            offer_sdp: SessionDescriptionProtocol::default() 
        };

        let response = handler.handle_client_message(msg);
        
        assert!(matches!(response, ServerResponse::Error(_)));
    }
}