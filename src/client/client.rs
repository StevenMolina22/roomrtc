use super::error::ClientError as Error;
use crate::{
    config::MediaConfig,
    ice::IceAgent,
    sdp::{Attribute, MediaDescription, SessionDescriptionProtocol},
};
use std::collections::HashSet;
use std::str::FromStr;

const MEDIA_TYPE: &str = "video";
const MEDIA_PROTOCOL: &str = "RTP/AVP";
// const MEDIA_FMT: u8 = 111;

/// High-level client that exposes SDP and ICE operations used by the UI
/// and signaling code.
///
/// `Client` holds a local `SessionDescriptionProtocol` and an `IceAgent`.
/// It can create an SDP offer, process remote offers/answers and drive
/// ICE connectivity checks.
pub struct Client {
    /// Local SDP state representing current media descriptions.
    pub sdp: SessionDescriptionProtocol,

    /// ICE agent responsible for gathering local candidates and
    /// performing connectivity checks with remote candidates.
    pub ice_agent: IceAgent,
}

impl Client {
    /// Create a new `Client` using the provided media port and codec
    /// configuration.
    ///
    /// Parameters:
    /// - `media_port`: UDP port where ICE candidate gathering is performed
    ///   and where the endpoint expects/receives RTP packets.
    /// - `media_config`: media stream configuration (payload type, codec
    ///   name and clock rate).
    ///
    /// This method performs the following steps:
    /// 1. Creates an `IceAgent` and calls `gather_candidates` to obtain
    ///    local candidates.
    /// 2. Builds a local `MediaDescription` and adds an `rtpmap` attribute
    ///    (and a `candidate` attribute if a local candidate is available).
    /// 3. Initializes the local `SessionDescriptionProtocol` and, if a
    ///    local candidate exists, sets the connection data (`c=`) to the
    ///    candidate's IP address.
    ///
    /// # Returns
    /// Returns a `Client` containing the local SDP and an `IceAgent`
    /// already configured with (potentially) gathered candidates.
    pub fn new(media_port: u16, media_config: MediaConfig) -> Self {
        let mut ice_agent = IceAgent::new();
        if ice_agent.gather_candidates(media_port).is_err() {
            panic!("Failed to gather ICE candidates");
        }

        let mut media_description = MediaDescription::new(
            MEDIA_TYPE.into(),
            media_port,
            MEDIA_PROTOCOL.into(),
            HashSet::from([media_config.rtp_payload_type]),
        );
        media_description
            .add_attribute(Attribute::RTPMap(
                media_config.rtp_payload_type,
                media_config.codec_name.clone(),
                media_config.clock_rate,
                None,
            ))
            .unwrap();

        if let Some(candidate) = ice_agent.get_local_candidate() {
            media_description
                .add_attribute(Attribute::Candidate(candidate.clone()))
                .unwrap();
        }

        let mut sdp = SessionDescriptionProtocol::new(vec![media_description]);

        if let Some(local_candidate) = ice_agent.get_local_candidate() {
            sdp.set_connection_data("IN", "IP4", local_candidate.address.clone().as_str());
        }

        Self { sdp, ice_agent }
    }

    /// Process an SDP offer string and return the generated SDP answer
    /// string. The method parses the incoming SDP, generates a local
    /// answer using the current local SDP state, and adds remote ICE
    /// candidates to the local `IceAgent` so connectivity checks can
    /// start.
    pub fn process_offer(&mut self, offer_str: &str) -> Result<String, Error> {
        let sdp_offer = SessionDescriptionProtocol::from_str(offer_str)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        let answer = self
            .sdp
            .create_answer(&sdp_offer)
            .map(|answer_sdp| answer_sdp.to_string())
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        self.process_remote_sdp(&sdp_offer)?;

        Ok(answer)
    }

    /// Process an SDP answer string (from the remote peer). This will
    /// parse the SDP and add any remote ICE candidates found to the
    /// local `IceAgent` and start connectivity checks.
    pub fn process_answer(&mut self, answer_str: &str) -> Result<(), Error> {
        let sdp_answer = SessionDescriptionProtocol::from_str(answer_str)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        self.process_remote_sdp(&sdp_answer)?;

        Ok(())
    }

    /// Return the local SDP offer string generated from the current
    /// `SessionDescriptionProtocol` state.
    pub fn get_offer(&self) -> String {
        self.sdp.to_string()
    }

    /// Internal helper that walks a remote `SessionDescriptionProtocol`,
    /// adds remote ICE candidates to the `IceAgent` and starts
    /// connectivity checks.
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
