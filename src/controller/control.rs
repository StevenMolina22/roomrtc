use crate::client_server_protocol::{ClientMessage, ClientResponse, ServerMessage, ServerResponse};
use crate::config::Config;
use crate::controller::{AppEvent, ControllerError as Error};
use crate::file::file_transferer::FileTransferer;
use crate::logger::Logger;
use crate::media::MediaPipeline;
use crate::media::frame_handler::Frame;
use crate::session::CallSession;
use crate::session::sdp::{Fingerprint, SessionDescriptionProtocol};
use crate::transport::MediaTransport;
use crate::user::UserStatus;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread;

/// Main controller for managing RTC calls and server communication.
///
/// `Controller` orchestrates all aspects of the RTC application, including:
/// - User authentication (login/logout, registration)
/// - Call initiation, acceptance, rejection, and termination
/// - Media transport and processing
/// - Event propagation to the UI layer
/// - Server communication and message handling
///
/// The controller maintains connections to both the signaling server (via TLS)
/// and coordinates RTP/RTCP media transport for active calls.
pub struct Controller {
    config: Arc<Config>,
    users_status: Arc<RwLock<HashMap<String, UserStatus>>>,
    token: Option<String>,
    username: Option<String>,
    logged_in: Arc<AtomicBool>,

    file_transferer: Option<FileTransferer>,
    transport: MediaTransport,
    call_session: CallSession,
    media_pipeline: MediaPipeline,

    event_tx: Sender<AppEvent>,

    client_server_stream: StreamOwned<ClientConnection, TcpStream>,
    logger: Logger,
}

impl Controller {
    /// Creates a new controller instance with TLS connection to the server.
    ///
    /// Initializes all media components (media pipeline, transport, and call session)
    /// and establishes a secure connection to the signaling server.
    ///
    /// # Parameters
    ///
    /// * `event_tx` - Channel sender for sending application events to the UI.
    /// * `config` - Application configuration containing server settings.
    /// * `sv_addr` - Socket address of the signaling server.
    /// * `logger` - Logger instance for recording operations.
    ///
    /// # Returns
    ///
    /// * `Ok(Controller)` - Successfully created controller.
    /// * `Err(Error)` - Failed to create controller or establish connection.
    pub fn new(
        event_tx: Sender<AppEvent>,
        config: &Arc<Config>,
        sv_addr: SocketAddr,
        logger: Logger,
    ) -> Result<Self, Error> {
        let client_server_stream = connect_tls(
            sv_addr,
            config.server.server_certification_file.clone(),
            config.server.server_name.clone(),
        )?;

        let media_pipeline = MediaPipeline::new(config, logger.context("MediaPipeline"));
        let transport = MediaTransport::new(config, logger.context("MediaTransport"))
            .map_err(|e| Error::MapError(e.to_string()))?;
        let socket_for_stun = transport
            .rtp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;
        let call_session = CallSession::new(socket_for_stun, config, logger.context("CallSession"))
            .map_err(|e| Error::MapError(e.to_string()))?;

        Ok(Self {
            config: Arc::clone(config),
            users_status: Arc::new(RwLock::new(HashMap::new())),
            token: None,
            username: None,
            logged_in: Arc::new(AtomicBool::new(false)),
            file_transferer: None,
            transport,
            call_session,
            media_pipeline,
            event_tx,
            client_server_stream,
            logger,
        })
    }

    // ---------------------------------------------------------------------------------------------------------------------------
    // FILES

    pub fn send_file(&mut self, file_path: &Path) -> Result<(), Error> {
        if let Some(transferer) = self.file_transferer.as_mut() {
            transferer
                .send_file(file_path)
                .map_err(|e| Error::MapError(e.to_string()))?;
        }
        Ok(())
    }

    pub fn accept_file(&mut self, id: u32, path: &Path) -> Result<(), Error> {
        if let Some(transferer) = self.file_transferer.as_mut() {
            transferer
                .accept_file_offer(id, path)
                .map_err(|e| Error::MapError(e.to_string()))?;
        }
        Ok(())
    }

    pub fn reject_file(&mut self, id: u32) -> Result<(), Error> {
        if let Some(transferer) = self.file_transferer.as_mut() {
            transferer
                .reject_file_offer(id)
                .map_err(|e| Error::MapError(e.to_string()))?;
        }
        Ok(())
    }

    // ---------------------------------------------------------------------------------------------------------------------------
    // CALLS

    /// Initiates an outgoing call to a peer.
    ///
    /// Sends a call request message to the server with the local SDP offer.
    /// The peer will receive a `CallIncoming` event to accept or reject the call.
    ///
    /// # Parameters
    ///
    /// * `peer_username` - Username of the peer to call.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Call request sent successfully.
    /// * `Err(Error)` - Failed to send call request.
    pub fn call(&mut self, peer_username: &str) -> Result<(), Error> {
        let token = self.get_token()?;
        let msg = ClientMessage::CallRequest {
            token: token.clone(),
            offer_sdp: self.call_session.get_offer(),
            to: peer_username.to_owned(),
        };
        match send_message(msg, &mut self.client_server_stream, &self.event_tx)? {
            ServerResponse::CallRequestOk => Ok(()),
            ServerResponse::CallRequestError(e) => Err(Error::CallError(e)),
            _ => Err(Error::BadResponse),
        }
    }

    /// Processes an accepted call and starts media transport.
    ///
    /// After the peer accepts a call, processes the SDP answer, performs ICE checks,
    /// and starts the media transport to exchange frames with the peer.
    ///
    /// # Parameters
    ///
    /// * `sdp_answer` - SDP answer from the peer.
    ///
    /// # Returns
    ///
    /// * `Ok((audio_rx, video_rx))` - Receivers for audio and video frames.
    /// * `Err(Error)` - Failed to process the answer or start media.
    pub fn get_in_call(
        &mut self,
        sdp_answer: SessionDescriptionProtocol,
    ) -> Result<(Receiver<Frame>, Receiver<Frame>), Error> {
        self.logger.info("Call request ok");
        self.call_session
            .process_answer(&sdp_answer)
            .map_err(|e| Error::MapError(e.to_string()))?;
        self.logger.info("Starting ICE checks...");
        self.call_session
            .start_ice_checks()
            .map_err(|e| Error::MapError(e.to_string()))?;

        self.logger.info("Joining call...");
        self.join_call()
    }

    /// Terminates an active call.
    ///
    /// Sends a hang-up message to the server, stops media components,
    /// and reinitializes call session for potential future calls.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Call terminated successfully.
    /// * `Err(Error)` - Failed to terminate call.
    pub fn hang_up(&mut self) -> Result<(), Error> {
        let msg = ClientMessage::CallHangup {
            token: self.get_token()?,
        };

        match send_message(msg, &mut self.client_server_stream, &self.event_tx)? {
            ServerResponse::CallHangUpOk => {}
            ServerResponse::CallHangUpError(e) => return Err(Error::CallError(e)),
            _ => return Err(Error::BadResponse),
        }

        self.stop_media_components()?;

        self.file_transferer = None;
        self.transport = MediaTransport::new(&self.config, self.logger.context("MediaTransport"))
            .map_err(|e| Error::MapError(e.to_string()))?;
        let socket_for_stun = self
            .transport
            .rtp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;
        self.call_session = CallSession::new(
            socket_for_stun,
            &self.config,
            self.logger.context("CallSession"),
        )
        .map_err(|e| Error::MapError(e.to_string()))?;

        Ok(())
    }

    /// Stops media pipeline and transport components.
    ///
    /// Gracefully stops all active media processing and network transport.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Media components stopped successfully.
    /// * `Err(Error)` - Failed to stop components.
    pub fn stop_media_components(&mut self) -> Result<(), Error> {
        self.media_pipeline.stop();
        self.transport
            .stop()
            .map_err(|e| Error::MapError(e.to_string()))
    }

    /// Accepts an incoming call from a peer.
    ///
    /// Processes the peer's SDP offer, sends an SDP answer to the server,
    /// performs ICE checks, and starts media transport.
    ///
    /// # Parameters
    ///
    /// * `to_usr` - Username of the peer who initiated the call.
    /// * `offer_sdp` - SDP offer from the peer.
    ///
    /// # Returns
    ///
    /// * `Ok((audio_rx, video_rx))` - Receivers for audio and video frames.
    /// * `Err(Error)` - Failed to accept call.
    pub fn accept_call(
        &mut self,
        to_usr: String,
        offer_sdp: &SessionDescriptionProtocol,
    ) -> Result<(Receiver<Frame>, Receiver<Frame>), Error> {
        let sdp_answer = self
            .call_session
            .process_offer(offer_sdp)
            .map_err(|e| Error::MapError(e.to_string()))?;
        let token = self.get_token()?;
        let msg = ClientMessage::CallAccept {
            from_usr: token.clone(),
            to_usr: to_usr.clone(),
            sdp_answer: sdp_answer.clone(),
        };

        let response = send_message(msg, &mut self.client_server_stream, &self.event_tx)?;

        match response {
            ServerResponse::CallAcceptOk => {
                self.logger.info("SDP Answer sent. Starting ICE checks...");
                self.call_session
                    .start_ice_checks()
                    .map_err(|e| Error::MapError(e.to_string()))?;

                self.logger.info("Joining call...");
                self.join_call()
            }
            ServerResponse::CallAcceptError(e) => Err(Error::CallError(e)),
            _ => Err(Error::BadResponse),
        }
    }

    /// Rejects an incoming call.
    ///
    /// Sends a call rejection message to the server to notify the peer.
    ///
    /// # Parameters
    ///
    /// * `to_usr` - Username of the peer whose call is being rejected.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Call rejection sent successfully.
    /// * `Err(Error)` - Failed to reject call.
    pub fn reject_call(&mut self, to_usr: String) -> Result<(), Error> {
        let token = self.get_token()?;
        let msg = ClientMessage::CallReject {
            from_usr: token,
            to_usr,
        };
        match send_message(msg, &mut self.client_server_stream, &self.event_tx)? {
            ServerResponse::CallRejectOk => {
                let event = AppEvent::CallRejected;
                send_event_or_log_out(&self.event_tx, event, &self.logged_in);
            }
            ServerResponse::CallRejectError(e) => return Err(Error::CallError(e)),
            _ => return Err(Error::BadResponse),
        }

        Ok(())
    }

    // Helper function to join an active call by setting up media transport and pipeline
    pub(crate) fn join_call(&mut self) -> Result<(Receiver<Frame>, Receiver<Frame>), Error> {
        let (remote_rtp_addr, remote_rtcp_addr, remote_sctp_addr) = self
            .get_remote_addresses()
            .map_err(|e| Error::MapError(e.to_string()))?;
        let remote_fingerprint = self
            .call_session
            .remote_fingerprint
            .clone()
            .ok_or(Error::NotLoggedInError)?;
        let handles = self
            .transport
            .start(
                remote_rtp_addr,
                remote_rtcp_addr,
                self.event_tx.clone(),
                self.call_session.local_setup_role,
                remote_fingerprint.clone(),
                &self.call_session.local_cert,
            )
            .map_err(|e| Error::MapError(e.to_string()))?;
        self.setup_file_transferer(
            remote_sctp_addr,
            remote_fingerprint.clone(),
            handles.is_connected.clone(),
        )?;

        self.media_pipeline
            .start(
                handles.rtp_tx,
                handles.rtp_rx,
                self.event_tx.clone(),
                handles.is_connected,
                handles.receiver_stats,
            )
            .map_err(|e| Error::MapError(e.to_string()))
    }

    // ---------------------------------------------------------------------------------------------------------------------------
    // AUTHENTICATION

    /// Registers a new user account.
    ///
    /// Sends a signup message to the server with the provided credentials.
    ///
    /// # Parameters
    ///
    /// * `username` - Desired username for the new account.
    /// * `password` - Password for the new account.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Registration successful.
    /// * `Err(Error)` - Registration failed.
    pub fn sign_up(&mut self, username: String, password: String) -> Result<(), Error> {
        let msg = ClientMessage::SignUp { username, password };
        match send_message(msg, &mut self.client_server_stream, &self.event_tx)? {
            ServerResponse::SignupOk => Ok(()),
            ServerResponse::SignupError(e) => Err(Error::MapError(e)),
            _ => Err(Error::BadResponse),
        }
    }

    /// Authenticates a user and starts the server communication thread.
    ///
    /// Sends login credentials to the server, receives a session token and list of online users,
    /// and spawns a background thread to handle incoming server messages.
    ///
    /// # Parameters
    ///
    /// * `username` - Username for authentication.
    /// * `password` - Password for authentication.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Login successful.
    /// * `Err(Error)` - Login failed.
    pub fn log_in(&mut self, username: &str, password: &str) -> Result<(), Error> {
        let msg = ClientMessage::LogIn {
            username: username.to_owned(),
            password: password.to_owned(),
        };
        match send_message(msg, &mut self.client_server_stream, &self.event_tx)? {
            ServerResponse::LoginOk(token, server_client_addr, users_status) => {
                self.users_status = Arc::new(RwLock::new(users_status));
                self.token = Some(token);
                self.logged_in.store(true, Ordering::SeqCst);
                self.username = Some(username.to_owned());
                self.start_server_receiver(username, server_client_addr)?;
                Ok(())
            }
            ServerResponse::LoginError(e) => Err(Error::LogInFailed(e)),
            _ => Err(Error::BadResponse),
        }
    }

    /// Logs out the current user.
    ///
    /// Sends a logout message to the server, invalidates the session token,
    /// and clears the user's status and token.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Logout successful.
    /// * `Err(Error)` - Logout failed.
    pub fn log_out(&mut self) -> Result<(), Error> {
        let token = self.get_token()?;

        let msg = ClientMessage::LogOut { token };
        match send_message(msg, &mut self.client_server_stream, &self.event_tx)? {
            ServerResponse::LogoutOk => {
                self.logged_in.store(false, Ordering::SeqCst);
                self.token = None;
                self.users_status = Arc::new(RwLock::new(HashMap::new()));

                Ok(())
            }
            ServerResponse::LogoutError(e) => Err(Error::LogOutFailed(e)),
            _ => Err(Error::BadResponse),
        }
    }

    // ---------------------------------------------------------------------------------------------------------------------------
    // SERVER COMMUNICATION THREAD

    // Spawns a background thread to receive and handle messages from the signaling server
    fn start_server_receiver(
        &self,
        username: &str,
        server_client_addr: SocketAddr,
    ) -> Result<(), Error> {
        let event_tx = self.event_tx.clone();
        let logged_in = self.logged_in.clone();
        let config = self.config.clone();
        let username = username.to_owned();
        let user_status = self.users_status.clone();

        let stream = connect_tls(
            server_client_addr,
            config.server.server_certification_file.clone(),
            config.server.server_name.clone(),
        )?;

        thread::spawn(move || {
            run_receiver_loop(logged_in, stream, event_tx, &username, user_status)
        });
        Ok(())
    }

    pub fn initial_handshake(&mut self) -> Result<(), Error> {
        let msg = ClientMessage::Hello;
        let ans = send_message(msg, &mut self.client_server_stream, &self.event_tx)?;

        match ans {
            ServerResponse::Welcome => Ok(()),
            ServerResponse::ServerFull => {
                if let Err(e) = self.event_tx.send(AppEvent::FullServerError) {
                    return Err(Error::MapError(e.to_string()));
                };
                Ok(())
            }
            _ => Err(Error::BadResponse),
        }
    }

    //----------------------------------------------------------------------------------------------------------------------------
    // PUBLIC ACCESSORS

    /// Retrieves the status of all known users.
    ///
    /// # Returns
    ///
    /// * `Ok(HashMap)` - Map of usernames to their current status.
    /// * `Err(Error)` - Failed to read user status (poisoned lock).
    pub fn get_users_status(&self) -> Result<HashMap<String, UserStatus>, Error> {
        let users_status = self.users_status.read().map_err(|_| Error::PoisonedLock)?;
        Ok(users_status.clone())
    }

    // Helper function to retrieve the session token if the user is logged in
    fn get_token(&self) -> Result<String, Error> {
        if let Some(token) = &self.token {
            Ok(token.clone())
        } else {
            self.logger
                .warn("No podes obtener token, no estas loggeado");
            Err(Error::NotLoggedInError)
        }
    }

    // Helper function to retrieve the current username
    pub(crate) fn get_username(&self) -> Result<String, Error> {
        self.get_token()
    }

    /// Toggles the microphone mute state.
    ///
    /// Delegates the action to the media pipeline. If audio is not active
    /// or initialized, this operation does nothing.
    pub fn toggle_audio(&self) {
        self.media_pipeline.toggle_audio();
    }

    fn get_remote_addresses(&mut self) -> Result<(SocketAddr, SocketAddr, SocketAddr), Error> {
        let pair = self
            .call_session
            .get_selected_pair()
            .map_err(|e| Error::MapError(e.to_string()))?;

        let remote_rtp_address: SocketAddr =
            format!("{}:{}", pair.remote.address, pair.remote.port)
                .parse()
                .map_err(Error::ParsingSocketAddressError)?;
        let remote_rtcp_address: SocketAddr =
            format!("{}:{}", pair.remote.address, pair.remote.port + 1)
                .parse()
                .map_err(Error::ParsingSocketAddressError)?;
        let remote_sctp_address: SocketAddr =
            format!("{}:{}", pair.remote.address, pair.remote.port + 2)
                .parse()
                .map_err(Error::ParsingSocketAddressError)?;

        Ok((remote_rtp_address, remote_rtcp_address, remote_sctp_address))
    }

    fn setup_file_transferer(
        &mut self,
        remote_sctp_addr: SocketAddr,
        remote_fingerprint: Fingerprint,
        connected: Arc<AtomicBool>,
    ) -> Result<(), Error> {
        let local_sctp_address = SocketAddr::new(
            self.transport.rtp_address.ip(),
            self.transport.rtp_address.port() + 2,
        );
        self.file_transferer = Some(
            FileTransferer::new(
                local_sctp_address,
                remote_sctp_addr,
                self.call_session.local_setup_role,
                remote_fingerprint,
                &self.call_session.local_cert,
                self.event_tx.clone(),
                connected,
                Arc::new(self.logger.clone()),
                self.config.clone(),
            )
            .map_err(|e| Error::MapError(e.to_string()))?,
        );
        Ok(())
    }
}

//----------------------------------------------------------------------------------------------------------------------------
// PRIVATE HELPER FUNCTIONS

fn run_receiver_loop(
    logged_in: Arc<AtomicBool>,
    mut stream: StreamOwned<ClientConnection, TcpStream>,
    event_tx: Sender<AppEvent>,
    username: &str,
    user_status: Arc<RwLock<HashMap<String, UserStatus>>>,
) -> Result<(), Error> {
    let mut buff = [0u8; 65535];
    while logged_in.load(Ordering::SeqCst) {
        match stream.read(&mut buff) {
            Ok(0) => break,
            Ok(size) => {
                if let Err(e) = handle_server_bytes(
                    size,
                    &buff,
                    &mut stream,
                    username,
                    event_tx.clone(),
                    user_status.clone(),
                ) {
                    send_event_or_log_out(&event_tx, AppEvent::Error(e.to_string()), &logged_in);
                    break;
                }
            }
            Err(e) if logged_in.load(Ordering::SeqCst) => {
                send_event_or_log_out(&event_tx, AppEvent::Error(e.to_string()), &logged_in);
                break;
            }
            Err(_) => break,
        }
    }
    Ok(())
}

// Sends an event to the UI and logs out if the channel is disconnected
fn send_event_or_log_out(
    event_tx: &Sender<AppEvent>,
    event: AppEvent,
    logged_in: &Arc<AtomicBool>,
) {
    if event_tx.send(event).is_err() {
        logged_in.store(false, Ordering::SeqCst);
    }
}

// Sends a client message and waits for the server response
fn send_message(
    msg: ClientMessage,
    stream: &mut StreamOwned<ClientConnection, TcpStream>,
    event_tx: &Sender<AppEvent>,
) -> Result<ServerResponse, Error> {
    if let Err(e) = stream.write_all(&msg.to_bytes()) {
        let _ = event_tx.send(AppEvent::FatalError);
        return Err(Error::IOError(e.to_string()));
    }

    let mut buff = [0u8; 1024];

    match stream.read(&mut buff) {
        Ok(0) => {
            let _ = event_tx.send(AppEvent::FatalError);
            Err(Error::IOError("Connection closed by server".to_string()))
        }
        Ok(size) => ServerResponse::from_bytes(&buff[..size]).ok_or(Error::BadResponse),
        Err(e) => {
            let _ = event_tx.send(AppEvent::FatalError);
            Err(Error::IOError(e.to_string()))
        }
    }
}

// Sends a client response to the server
fn send_response(
    response: ClientResponse,
    stream: &mut StreamOwned<ClientConnection, TcpStream>,
) -> Result<(), Error> {
    stream
        .write_all(&response.to_bytes())
        .map_err(|e| Error::MapError(e.to_string()))
}

// Processes a server message and generates the appropriate response
fn get_response_for_server_message(
    server_msg: ServerMessage,
    event_tx: &Sender<AppEvent>,
    username: String,
    user_status: Arc<RwLock<HashMap<String, UserStatus>>>,
) -> Result<Option<ClientResponse>, Error> {
    match server_msg {
        ServerMessage::UsernameRequest => Ok(Some(ClientResponse::Username(username))),
        ServerMessage::UserStatusUpdate(username, status) => {
            update_status(username, status, user_status)?;
            Ok(None)
        }
        ServerMessage::CallIncoming {
            from_usr,
            offer_sdp,
        } => {
            if let Err(e) = event_tx.send(AppEvent::CallIncoming(from_usr, offer_sdp)) {
                return Err(Error::MapError(e.to_string()));
            };
            Ok(None)
        }
        ServerMessage::CallAccepted {
            from_usr,
            sdp_answer,
        } => {
            if let Err(e) = event_tx.send(AppEvent::CallAccepted(
                sdp_answer,
                username.clone(),
                from_usr,
            )) {
                return Err(Error::MapError(e.to_string()));
            }
            Ok(None)
        }
        ServerMessage::CallRejected => {
            if let Err(e) = event_tx.send(AppEvent::CallRejected) {
                return Err(Error::MapError(e.to_string()));
            }
            Ok(None)
        }
        ServerMessage::Error(e) => {
            if let Err(e) = event_tx.send(AppEvent::Error(e)) {
                return Err(Error::MapError(e.to_string()));
            }
            Ok(None)
        }
    }
}

// Updates the status of a user in the shared status map
fn update_status(
    username: String,
    status: UserStatus,
    user_status: Arc<RwLock<HashMap<String, UserStatus>>>,
) -> Result<(), Error> {
    let mut user_status = match user_status.write() {
        Ok(user_status) => user_status,
        Err(_) => return Err(Error::PoisonedLock),
    };

    user_status.insert(username, status);
    Ok(())
}

/// Establishes a secure TLS connection to the signaling server.
///
/// Creates a TCP connection to the server and wraps it with TLS encryption using
/// the provided certificate and server name for verification.
///
/// # Parameters
///
/// * `server_addr` - Socket address of the server.
/// * `ca_cert_path` - Path to the CA certificate file for server verification.
/// * `server_name_str` - Expected server name for TLS verification.
///
/// # Returns
///
/// * `Ok(stream)` - Secure TLS stream to the server.
/// * `Err(Error)` - Failed to establish connection or configure TLS.
pub fn connect_tls(
    server_addr: SocketAddr,
    ca_cert_path: String,
    server_name_str: String,
) -> Result<StreamOwned<ClientConnection, TcpStream>, Error> {
    let mut root_store = RootCertStore::empty();
    let f = File::open(ca_cert_path).map_err(|e| Error::MapError(e.to_string()))?;
    let mut reader = BufReader::new(f);

    for cert in rustls_pemfile::certs(&mut reader) {
        let cert = cert.map_err(|e| Error::MapError(e.to_string()))?;
        root_store
            .add(cert)
            .map_err(|e| Error::MapError(e.to_string()))?;
    }

    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    let config = Arc::new(config);

    let server_name = ServerName::try_from(server_name_str)
        .map_err(|_| Error::MapError("Invalid server name".to_string()))?;

    let tcp_stream =
        TcpStream::connect(server_addr).map_err(|e| Error::ConnectionSocketError(e.to_string()))?;

    let tls_conn =
        ClientConnection::new(config, server_name).map_err(|e| Error::MapError(e.to_string()))?;
    let tls_stream = StreamOwned::new(tls_conn, tcp_stream);

    Ok(tls_stream)
}

fn handle_server_bytes(
    size: usize,
    buff: &[u8],
    stream: &mut StreamOwned<ClientConnection, TcpStream>,
    username: &str,
    event_tx: Sender<AppEvent>,
    users_status: Arc<RwLock<HashMap<String, UserStatus>>>,
) -> Result<(), Error> {
    let server_msg = ServerMessage::from_bytes(&buff[..size])
        .ok_or_else(|| Error::MapError("Failed to parse message".to_string()))?;

    if let Some(response) = get_response_for_server_message(
        server_msg,
        &event_tx,
        username.to_string(),
        users_status.clone(),
    )
    .map_err(|e| Error::MapError(e.to_string()))?
    {
        send_response(response, stream).map_err(|e| Error::MapError(e.to_string()))?;
    }
    Ok(())
}
