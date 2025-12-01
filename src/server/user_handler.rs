use super::{ServerError as Error, operating_server::OperatingServer};
use crate::client_server_protocol::{ClientMessage, ServerResponse};
use crate::config::Config;
use crate::user::UserData;
use rustls::{ServerConnection, StreamOwned};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

pub struct UserHandler {
    op_server: OperatingServer,
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
    ) -> Self {
        Self {
            op_server: OperatingServer::new(users, users_connected, server_client_socket_addr, config.server.users_file.clone()),
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
                    self.op_server.make_user_offline()?;
                    return Ok(()) },
                Ok(n) => ClientMessage::from_bytes(&buff[0..n]),
                Err(e) => {
                    self.op_server.make_user_offline()?;
                    return Err(Error::ConnectionError(e.to_string()))
                },
            };
            
            let sv_response = match client_event {
                Some(event) => {
                    self.handle_client_message(event)
                },
                None => ServerResponse::BadMessage
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
            ClientMessage::LogIn { username, password } => {
                self.op_server.login_user(username, password)
            }

            ClientMessage::SignUp { username, password } => {
                self.op_server.signup_user(username, password)
            }

            ClientMessage::LogOut { token } => {
                self.op_server.logout_user(token)
            },

            ClientMessage::CallRequest {
                token,
                offer_sdp,
                to,
            } => {
                self.op_server.call_request(token, to, offer_sdp)
            },

            ClientMessage::CallHangup { token } => {
                self.op_server.call_hangup(token)
            },
        }
    }
}

/// Sends a serialized `ServerResponse` over the TCP stream.
///
/// Any I/O error is logged but not returned to the caller.
fn send_response(stream: &mut StreamOwned<ServerConnection, TcpStream>, response: ServerResponse) {
    stream.write_all(&response.to_bytes());
}
