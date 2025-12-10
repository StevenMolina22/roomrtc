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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::user::user_status::UserStatus;

    #[test]
    fn test_new_user_creation() {
        let username = "alice".to_string();
        let password = "secret123".to_string();
        let status = UserStatus::Available;

        let user = UserData::new(username.clone(), password.clone(), status.clone());

        assert_eq!(user.username, username);
        assert_eq!(user.password, password);
        assert_eq!(user.status, status);
        
        assert!(user.server_client_stream.is_none());
    }

    #[test]
    fn test_update_status() {
        let mut user = UserData::new(
            "bob".to_string(), 
            "pass".to_string(), 
            UserStatus::Offline
        );

        assert_eq!(user.status, UserStatus::Offline);

        let new_status = UserStatus::Occupied("Coding".to_string());
        user.update_status(new_status.clone());

        assert_eq!(user.status, new_status);
    }

    #[test]
    fn test_user_data_clone() {
        let user = UserData::new(
            "charlie".to_string(), 
            "1234".to_string(), 
            UserStatus::Available
        );
        
        let mut user_clone = user.clone();
        
        assert_eq!(user.username, user_clone.username);
        assert_eq!(user.status, user_clone.status);

        user_clone.update_status(UserStatus::Offline);
        assert_ne!(user.status, user_clone.status);
        assert_eq!(user.status, UserStatus::Available);
    }
}