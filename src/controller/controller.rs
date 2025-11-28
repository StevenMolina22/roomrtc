use std::sync::{Arc};
use std::sync::mpsc::{Receiver, Sender};
use crate::config::Config;
use crate::controller::{AppEvent, ControllerError as Error};
use crate::media::frame_handler::Frame;
use crate::media::MediaPipeline;
use crate::session::CallSession;
use crate::transport::MediaTransport;

pub struct Controller {
    _config: Arc<Config>,
    // pub(crate) users_status: Arc<RwLock<HashMap<String, UserStatus>>>,
    // client_server_stream: TcpStream,

    transport: MediaTransport,
    call_session: CallSession,
    media_pipeline: MediaPipeline,

}

impl Controller {
    pub fn new(_event_tx: Sender<AppEvent>, config: &Arc<Config>) -> Result<Self, Error> {
        // let client_server_stream = TcpStream::connect(&cfg.signaling_server.client_server_address)
        //     .map_err(|_| Error::ConnectingToServerFailed)?;
        
        // Le pongo src 0 pero le podriamos poner un token o algo
        let media_pipeline = MediaPipeline::new(config, 0);
        let transport = MediaTransport::new(config).map_err(|e| Error::MapError(e.to_string()))?;
        let call_session = CallSession::new(transport.rtp_address.port(), config).map_err(|e| Error::MapError(e.to_string()))?;

        Ok(Self {
            _config: Arc::clone(config),
            // users_status: Arc::new(RwLock::new(HashMap::new())),
            // client_server_stream,
            transport,
            call_session,
            media_pipeline,
        })
    }


    pub fn get_sdp_offer(&self) -> String {
        self.call_session.get_offer()
    }

    pub fn process_offer(&mut self, offer_sdp: &String) -> Result<String, Error> {
        self.call_session.process_offer(offer_sdp.as_str()).map_err(|e| Error::MapError(e.to_string()))
    }

    pub fn process_answer(&mut self, answer_sdp: &String) -> Result<(), Error> {
        self.call_session.process_answer(answer_sdp.as_str()).map_err(|e| Error::MapError(e.to_string()))
    }

    pub fn start_call(&mut self) -> Result<(Receiver<Frame>, Receiver<Frame>), Error> {
        // let remote_rtp_address = self.call_session.get_remote_address().map_err(|e| Error::MapError(e.to_string()))?;
        let pair = self.call_session.get_selected_pair().map_err(|e| Error::MapError(e.to_string()))?;
        
        let remote_rtp_address: std::net::SocketAddr =
            format!("{}:{}", pair.remote.address, pair.remote.port)
                .parse()
                .map_err(Error::ParsingSocketAddressError)?;
        let remote_rtcp_address: std::net::SocketAddr =
            format!("{}:{}", pair.remote.address, pair.remote.port + 1)
                .parse()
                .map_err(Error::ParsingSocketAddressError)?;
        
        let (local_to_remote_rtp_tx, remote_to_local_rtp_rx) = self.transport.start(remote_rtp_address, remote_rtcp_address).map_err(|e| Error::MapError(e.to_string()))?;
        
        self.media_pipeline.start(local_to_remote_rtp_tx, remote_to_local_rtp_rx).map_err(|e| Error::MapError(e.to_string()))
    }

    pub fn hang_down(&mut self) -> Result<(), Error> {
        self.media_pipeline.stop();
        self.transport.stop().map_err(|e| Error::MapError(e.to_string()))
    }

    // pub fn sign_up(&self, username: String, password: String) -> Result<(), Error> {
    //     self.send_message()
    // }
    //
    // pub fn sign_in(&self, username: String, password: String) -> Result<(), Error> {}
    //
    // pub fn sign_out(&self, username: String, password: String) {}

    // pub fn hang_up(&mut self) -> Result<(), Error> {
    // }
}
