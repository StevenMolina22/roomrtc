use super::{ServerError, UserHandler};
use super::error::ServerError as Error;
use crate::client_server_protocol::{ClientResponse, ServerMessage};
use crate::config::Config;
use crate::logger::Logger;
use crate::user::{UserData, UserStatus};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;

/// Central server structure that holds global state and coordinates
/// communication with all clients.
pub struct CentralServer {
    config: Arc<Config>,
    tls_config: Arc<ServerConfig>,
    users: Arc<RwLock<HashMap<String, UserData>>>,
    users_connected: Arc<AtomicUsize>,
    on: Arc<AtomicBool>,
    server_client_socket_addr: SocketAddr,

    logger: Logger,
}

impl CentralServer {
    /// Creates a new `CentralServer` and loads users from the configured file.
    ///
    /// # Errors
    /// Returns an error if the user file cannot be opened or parsed.
    pub fn new(config: Arc<Config>, logger: Logger) -> Result<Self, Error> {
        let (certs, key) = generate_certs_and_key(config.clone())?;
        let users = load_users_from_mem(&config.server.users_file)?;

        let tls_config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, key)
                .map_err(|e| Error::MapError(e.to_string()))?,
        );

        let local_ip = get_server_ip().map_err(|e| Error::MapError(e.to_string()))?;
        let client_server_socket_addr = SocketAddr::from_str(&format!("{}:{}", local_ip, 8080))
            .map_err(|e| Error::MapError(e.to_string()))?;
        let server_client_socket_addr = SocketAddr::from_str(&format!("{}:{}", local_ip, 8081))
            .map_err(|e| Error::MapError(e.to_string()))?;

        println!(
            "Server listening on {}:{}",
            client_server_socket_addr.ip(),
            client_server_socket_addr.port()
        );

        Ok(Self {
            config,
            tls_config,
            users: Arc::new(RwLock::new(users)),
            users_connected: Arc::new(AtomicUsize::new(0)),
            on: Arc::new(AtomicBool::new(false)),
            server_client_socket_addr,
            logger,
        })
    }

    /// Starts the main TCP listener in a dedicated thread.
    /// Each incoming connection is handled in its own worker thread.
    pub fn start(&mut self) -> Result<(), Error> {
        self.on.store(true, Ordering::SeqCst);
        self.spawn_server_client_thread()?;
        self.await_for_connections()?;

        let mut line = String::new();
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();

        loop {
            line.clear();

            match reader.read_line(&mut line) {
                Ok(0) => {
                    self.on.store(false, Ordering::SeqCst);
                    break;
                }
                Ok(_) => {}
                Err(_) => {
                    self.on.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }
        Ok(())
    }
    // Spawns the initial TCP listener thread.
    // Accepts incoming client connections and spawns one worker per connection,
    // sharing the user map and the connected-user counter across threads.
    fn await_for_connections(&mut self) -> Result<(), Error> {
        let config = self.config.clone();
        let tls_config = self.tls_config.clone();
        let users = self.users.clone();
        let users_connected = self.users_connected.clone();
        let on = self.on.clone();
        let server_client_socket_addr = self.server_client_socket_addr;
        let max_users = self.config.server.max_amount_of_users_connected;

        let logger = self.logger.clone();

        thread::spawn(move || {
            let cs_listener = if let Ok(l) = TcpListener::bind(&config.server.client_server_addr) {
                l
            } else {
                on.store(false, Ordering::SeqCst);
                return;
            };

            for stream in cs_listener.incoming() {
                if !on.load(Ordering::SeqCst) {
                    break;
                }
                if users_connected.load(Ordering::SeqCst) == config.server.max_amount_of_users_connected {
                    println!("Max amount of users connected reached. Abort connection");
                    continue;
                }
                users_connected.fetch_add(1, Ordering::SeqCst);
                println!("User Connected. Total: {}", users_connected.load(Ordering::SeqCst));
                let stream = match stream {
                    Ok(s) => s,
                    Err(_) => {
                        continue;
                    }
                };
                let tls_config = tls_config.clone();
                let users_connected = users_connected.clone();
                let config = config.clone();
                let users = users.clone();
                let on = on.clone();
                let server_client_socket_addr = server_client_socket_addr;

                let thread_logger = logger.context("UserHandler");

                thread::spawn(move || {
                    let tls_conn = match ServerConnection::new(tls_config) {
                        Ok(conn) => conn,
                        Err(_) => return,
                    };
                    let tls_stream = StreamOwned::new(tls_conn, stream);

                    let mut user_handler = UserHandler::new(
                        users,
                        config,
                        server_client_socket_addr,
                        max_users,
                        thread_logger,
                    );
                    match user_handler.handle_client(tls_stream, on) {
                        Ok(())|Err(ServerError::ConnectionError(_)) => users_connected
                            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                                if v == 0 { Some(0) } else { Some(v - 1) }
                            })
                            .ok(),
                        _ => Some(0),
                    };
                    println!("User disconnected. Total: {}", users_connected.load(Ordering::SeqCst));
                });
            }
        });
        Ok(())
    }

    /// Spawns the dedicated server→client listener thread.
    ///
    /// This thread listens on the `server_client_addr` port, which is the
    /// channel used by the server to **push events to clients** (e.g.
    /// `CallIncoming`, status updates, etc.).
    ///
    /// For each incoming TCP connection:
    /// - a new worker thread is spawned,
    /// - the connection is passed to `map_stream_to_user`,
    /// - the stream is associated with a username once the client provides it.
    ///
    /// This listener operates independently of the main client→server channel.
    fn spawn_server_client_thread(&mut self) -> Result<(), Error> {
        let sc_addr = self.config.server.server_client_addr.clone();
        let tls_config = self.tls_config.clone();
        let users = self.users.clone();

        thread::spawn(move || {
            let listener = match TcpListener::bind(sc_addr) {
                Ok(l) => l,
                Err(_) => return,
            };

            for incoming in listener.incoming() {
                let stream = match incoming {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                let tls_config = tls_config.clone();
                let users = users.clone();

                thread::spawn(move || {
                    let tls_conn = match ServerConnection::new(tls_config) {
                        Ok(conn) => conn,
                        Err(_) => return,
                    };
                    let tls_stream = StreamOwned::new(tls_conn, stream);
                    map_stream_to_user(users, tls_stream);
                });
            }
        });
        Ok(())
    }
}

// Loads user credentials from disk; creates an empty file if it is missing.
fn load_users_from_mem(filename: &String) -> Result<HashMap<String, UserData>, Error> {
    let file = if let Ok(f) = File::open(filename) {
        f
    } else {
        OpenOptions::new()
            .truncate(true)
            .write(true)
            .open(filename)
            .map_err(|e| Error::MapError(e.to_string()))?;

        return Ok(HashMap::new());
    };

    let reader = BufReader::new(file);
    let mut users = HashMap::new();

    for line in reader.lines() {
        if let Ok(l) = line
            && let Some((username, password)) = l.split_once(':')
        {
            let data = UserData {
                username: username.to_string(),
                password: password.to_string(),
                status: UserStatus::Offline,
                server_client_stream: None,
            };
            users.insert(username.to_string(), data);
        }
    }

    Ok(users)
}

// Reads certificate and private key files from disk, returning them ready for TLS setup.
fn generate_certs_and_key(
    config: Arc<Config>,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), Error> {
    let mut certs = Vec::new();
    let cert_iter = CertificateDer::pem_file_iter(config.server.server_certification_file.clone())
        .map_err(|e| Error::MapError(e.to_string()))?;
    for item in cert_iter {
        match item {
            Ok(item) => certs.push(item),
            Err(e) => return Err(Error::MapError(e.to_string())),
        }
    }
    let key = PrivateKeyDer::from_pem_file(config.server.server_private_key_file.clone())
        .map_err(|e| Error::MapError(e.to_string()))?;

    Ok((certs, key))
}

/// Maps an incoming TCP stream from the server→client channel to a user.
///
/// This function is invoked when a client connects on the
/// `server_client_addr` port. The server requests the client's username,
/// waits for a `ClientResponse::Username`, and stores the received stream
/// inside the corresponding `UserData` entry.
///
/// This stream is later used to deliver server→client events such as
/// `CallIncoming`.
///
/// If anything fails (invalid packet, unknown user, I/O error), the
/// mapping attempt is silently aborted.
pub fn map_stream_to_user(
    users: Arc<RwLock<HashMap<String, UserData>>>,
    mut stream: StreamOwned<ServerConnection, TcpStream>,
) {
    if stream.write_all(&ServerMessage::UsernameRequest.to_bytes()).is_err() {
        return;
    }

    let mut buff = [0u8; 1024];
    let n = match stream.read(&mut buff) {
        Ok(0) => return,
        Ok(n) => n,
        Err(_) => return,
    };

    let response = match ClientResponse::from_bytes(&buff[..n]) {
        Some(r) => r,
        None => return,
    };

    let username = match response {
        ClientResponse::Username(name) => name
    };

    let mut users = match users.write() {
        Ok(u) => u,
        Err(_) => return,
    };
    if let Some(mut user_data) = users.get(&username).cloned() {
        user_data.update_server_client_stream(stream);
        users.insert(username, user_data);
    }
}

// Retrieves the first non-loopback IPv4 address from available interfaces.
fn get_server_ip() -> Result<String, Error> {
    let interfaces = if_addrs::get_if_addrs().map_err(|e| Error::MapError(e.to_string()))?;

    for interface in interfaces {
        if !interface.is_loopback()
            && let std::net::IpAddr::V4(ipv4) = interface.addr.ip()
        {
            return Ok(ipv4.to_string());
        }
    }

    Err(Error::IPNotFound(String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::Path;

    fn with_temp_file<F>(content: &str, test_fn: F)
    where
        F: FnOnce(&String),
    {
        let filename = format!("test_users_{}.txt", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0));
        
        if let Ok(mut file) = File::create(&filename) {
            let _ = file.write_all(content.as_bytes());
        }

        test_fn(&filename);

        let _ = fs::remove_file(filename);
    }


    #[test]
    fn test_load_users_valid_entries() {
        let content = "alice:1234\nbob:secret\ncharlie:pass";
        
        with_temp_file(content, |filename| {
            let result = load_users_from_mem(filename);
            
            match result {
                Ok(users) => {
                    assert_eq!(users.len(), 3);
                    
                    if let Some(u) = users.get("alice") {
                        assert_eq!(u.password, "1234");
                        assert_eq!(u.status, UserStatus::Offline);
                    } else {
                        assert!(false, "Falta el usuario alice");
                    }

                    if let Some(u) = users.get("bob") {
                        assert_eq!(u.password, "secret");
                    } else {
                        assert!(false, "Falta el usuario bob");
                    }
                },
                Err(e) => assert!(false, "Falló la carga: {}", e),
            }
        });
    }

    #[test]
    fn test_load_users_ignores_malformed_lines() {
        let content = "valid:pass\nmalformed\n\ncomplex:pass:word";

        with_temp_file(content, |filename| {
            let result = load_users_from_mem(filename);

            if let Ok(users) = result {
                assert_eq!(users.len(), 2);
                assert!(users.contains_key("valid"));
                
                if let Some(u) = users.get("complex") {
                    assert_eq!(u.password, "pass:word"); 
                }
            } else {
                assert!(false, "No debería fallar por líneas mal formadas, solo ignorarlas");
            }
        });
    }

    #[test]
    fn test_load_users_creates_file_if_missing() {
        let filename = "test_users_missing.txt";
        let _ = fs::remove_file(filename);

        let result = load_users_from_mem(&filename.to_string());

        match result {
            Ok(users) => {
                assert!(users.is_empty());
                assert!(Path::new(filename).exists(), "El archivo debería haber sido creado");
            },
            Err(e) => assert!(false, "Error inesperado: {}", e),
        }

        let _ = fs::remove_file(filename);
    }


    #[test]
    fn test_get_server_ip_returns_valid_result() {
        let result = get_server_ip();
        
        match result {
            Ok(ip) => {
                assert!(!ip.is_empty());
                assert!(ip.contains('.'));
            },
            Err(Error::IPNotFound(_)) => {
                assert!(true);
            },
            Err(e) => assert!(false, "Error desconocido obteniendo IP: {}", e),
        }
    }
}