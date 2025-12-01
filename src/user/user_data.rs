use super::user_status::UserStatus;
use rustls::{ServerConnection, StreamOwned};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct UserData {
    pub username: String,
    pub password: String,
    pub status: UserStatus,
    pub server_client_stream: Option<Arc<Mutex<StreamOwned<ServerConnection, TcpStream>>>>,
}

impl UserData {
    #[must_use]
    pub const fn new(username: String, password: String, status: UserStatus) -> Self {
        Self {
            username,
            password,
            status,
            server_client_stream: None,
        }
    }

    pub fn update_status(&mut self, status: UserStatus) {
        self.status = status;
    }

    pub fn update_server_client_stream(
        &mut self,
        stream: StreamOwned<ServerConnection, TcpStream>,
    ) {
        self.server_client_stream = Some(Arc::new(Mutex::new(stream)));
    }
}
