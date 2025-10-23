use crate::client::error::ClientError as Error;
use crate::ice::IceAgent;
use crate::rtp::rtp_communicator::{RtpReceiver, RtpSender};
use crate::sdp::{Attribute, MediaDescription, SessionDescriptionProtocol};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

const MEDIA_TYPE: &str = "video";
const MEDIA_PORT: u16 = 4000;
const MEDIA_PROTOCOL: &str = "RTP/AVP";
const MEDIA_FMT: u8 = 111;

pub struct VideoFrame {
    pub rgb_data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct Client {
    pub sdp: SessionDescriptionProtocol,
    pub ice_agent: IceAgent,
    // media
    pub local_video_sender: Sender<VideoFrame>, // For "self-view"
    pub remote_video_sender: Sender<VideoFrame>, // For decoded peer video
    pub rtp_sender: Option<Arc<Mutex<RtpSender>>>, // Will be set after handshake
    pub rtp_receiver: Option<Arc<Mutex<RtpReceiver>>>, // Will be set after handshake
}

impl Client {
    pub fn new(
        local_video_sender: Sender<VideoFrame>,
        remote_video_sender: Sender<VideoFrame>,
    ) -> Self {
        // Create ICE agent and gather network candidates
        let mut ice_agent = IceAgent::new();
        if ice_agent.gather_candidates(MEDIA_PORT).is_err() {
            panic!("Failed to gather ICE candidates");
        }

        // Build media description for video
        let mut media_description = MediaDescription::new(
            MEDIA_TYPE.into(),
            MEDIA_PORT,
            MEDIA_PROTOCOL.into(),
            HashSet::from([MEDIA_FMT]),
        );
        media_description
            .add_attribute(Attribute::RTPMap(
                111,
                "OPUS".into(),
                48000,
                Some("2".into()),
            ))
            .unwrap();

        // Add our ICE candidate to the media description
        if let Some(candidate) = ice_agent.get_local_candidate() {
            media_description
                .add_attribute(Attribute::Candidate(candidate.clone()))
                .unwrap();
        }

        // Create SDP with our media description
        let mut sdp = SessionDescriptionProtocol::new(vec![media_description]);

        // Set connection info using our local IP
        if let Some(local_candidate) = ice_agent.get_local_candidate() {
            sdp.set_connection_data("IN", "IP4", local_candidate.address.clone().as_str());
        }

        Self {
            sdp,
            ice_agent,
            local_video_sender,
            remote_video_sender,
            rtp_sender: None,
            rtp_receiver: None,
        }
    }

    /// Processes a remote offer, starts ICE, and returns an answer string.
    pub fn process_offer(&mut self, offer_str: &str) -> Result<String, Error> {
        let sdp_offer = SessionDescriptionProtocol::from_str(offer_str)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        // Process remote candidates
        self.process_remote_sdp(&sdp_offer)?;

        // Start media on success
        if let Some(pair) = self.ice_agent.get_selected_pair() {
            self.start_media_threads(pair.clone())?;
        }

        // Create and return answer
        self.sdp
            .create_answer(&sdp_offer)
            .map(|answer_sdp| answer_sdp.to_string())
            .map_err(|e| Error::SdpCreationError(e.to_string()))
    }

    /// Processes a remote answer and starts ICE.
    pub fn process_answer(&mut self, answer_str: &str) -> Result<(), Error> {
        let sdp_answer = SessionDescriptionProtocol::from_str(answer_str)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        // Process remote candidates
        self.process_remote_sdp(&sdp_answer);

        if let Some(pair) = self.ice_agent.get_selected_pair() {
            self.start_media_threads(pair.clone())?;
        }

        Ok(())
    }

    /// Returns this client's SDP offer string.
    pub fn get_offer(&self) -> String {
        self.sdp.to_string()
    }

    /// Private helper to extract ICE candidates from any SDP and start ICE.
    fn process_remote_sdp(&mut self, sdp: &SessionDescriptionProtocol) -> Result<(), Error> {
        for md in &sdp.media_descriptions {
            for candidate in md.get_candidates() {
                self.ice_agent
                    .add_remote_candidate(candidate.clone())
                    .map_err(|e| Error::IceConnectionError(e.to_string()))?;
            }
        }

        self.ice_agent
            .start_connectivity_checks()
            .map_err(|e| Error::IceConnectionError(e.to_string()))
    }
}
