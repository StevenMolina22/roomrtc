use crate::config::{Config};
use std::collections::HashSet;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use crate::session::error::CallSessionError as Error;
use crate::session::ice::IceAgent;
use crate::session::sdp::{Attribute, MediaDescription, SessionDescriptionProtocol};

/// High-level session that exposes SDP and ICE operations used by the UI
/// and signaling code.
///
/// `CallSession` holds a local `SessionDescriptionProtocol` and an `IceAgent`.
/// It can create an SDP offer, process remote offers/answers and drive
/// ICE connectivity checks.
pub struct CallSession {
    /// Local SDP state representing current media descriptions.
    pub sdp: SessionDescriptionProtocol,

    /// ICE agent responsible for gathering local candidates and
    /// performing connectivity checks with remote candidates.
    pub ice_agent: IceAgent,
}

impl CallSession {
    /// Create a new `CallSession` using the provided media port and codec
    /// configuration.
    /// - `ice_config`: ICE configuration for candidate creation.
    /// - `sdp_config`: SDP session-level configuration values.
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
    /// Returns a `CallSession` containing the local SDP and an `IceAgent`
    /// already configured with (potentially) gathered candidates.
    pub fn new(
        media_port: u16,
        config: &Arc<Config>
    ) -> Result<Self, Error> {
        let mut ice_agent = IceAgent::new();
        ice_agent
            .gather_candidates(media_port, &config.ice)
            .map_err(|e| {
                Error::IceConnectionError(format!("Failed to gather ICE candidates: {e}"))
            })?;

        let mut media_description = MediaDescription::new(
            config.media.media_type.clone(),
            media_port,
            config.media.media_protocol.clone(),
            HashSet::from([config.media.rtp_payload_type]),
        );
        media_description
            .add_attribute(Attribute::RTPMap(
                config.media.rtp_payload_type,
                config.media.codec_name.clone(),
                config.media.clock_rate,
                None,
            ))
            .map_err(|e| {
                Error::SdpCreationError(format!("Failed to add RTPMap attribute: {e}"))
            })?;

        if let Some(candidate) = ice_agent.get_local_candidate() {
            media_description
                .add_attribute(Attribute::Candidate(candidate.clone()))
                .map_err(|e| {
                    Error::SdpCreationError(format!("Failed to add Candidate attribute: {e}"))
                })?;
        }

        let mut sdp = SessionDescriptionProtocol::new(vec![media_description], &config.sdp);

        if let Some(local_candidate) = ice_agent.get_local_candidate() {
            sdp.set_connection_data("IN", "IP4", local_candidate.address.clone().as_str());
        }

        Ok(Self { sdp, ice_agent })
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
    #[must_use]
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



    pub fn get_remote_address(&self) -> Result<SocketAddr, Error> {
        let pair = self
            .ice_agent
            .get_selected_pair()
            .map_err(|e| Error::IceConnectionError(e.to_string()))?;

        let ip: IpAddr = pair.remote.address
            .parse()
            .map_err(|_| Error::BadAddress)?;

        Ok(SocketAddr::from((ip, pair.remote.port)))
    }
}