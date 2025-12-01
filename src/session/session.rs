use crate::{
    config::{IceConfig, MediaConfig, SdpConfig},
    ice::IceAgent,
    sdp::{Attribute, MediaDescription, SessionDescriptionProtocol},
};
use std::collections::HashSet;
use std::str::FromStr;

use crate::session::error::SessionError as Error;
use crate::session::sdp::{Attribute, MediaDescription, SessionDescriptionProtocol};

/// High-level session that exposes SDP and ICE operations used by the UI
/// and signaling code.
///
/// `Session` holds a local `SessionDescriptionProtocol` and an `IceAgent`.
/// It can create an SDP offer, process remote offers/answers and drive
/// ICE connectivity checks.
pub struct Session {
    /// Local SDP state representing current media descriptions.
    pub sdp: SessionDescriptionProtocol,

    /// ICE agent responsible for gathering local candidates and
    /// performing connectivity checks with remote candidates.
    pub ice_agent: IceAgent,
}

impl Session {
    /// Create a new `Session` using the provided media port and codec
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
    /// Returns a `Session` containing the local SDP and an `IceAgent`
    /// already configured with (potentially) gathered candidates.
    pub fn new(
        socket: std::net::UdpSocket,
        media_config: &MediaConfig,
        ice_config: &IceConfig,
        sdp_config: &SdpConfig,
    ) -> Result<Self, Error> {
        let mut ice_agent = IceAgent::new();
        ice_agent
            .gather_candidates(&socket, ice_config)
            .map_err(|e| {
                Error::IceConnectionError(format!("Failed to gather ICE candidates: {e}"))
            })?;

        let mut media_description = MediaDescription::new(
            media_config.media_type.clone(),
            socket.local_addr().map_err(|e| ClientError::IceConnectionError(format!("Failed to get local socket address: {e}")))?.port(),
            media_config.media_protocol.clone(),
            HashSet::from([media_config.rtp_payload_type]),
        );

        media_description
            .add_attribute(Attribute::RTPMap(
                media_config.rtp_payload_type,
                media_config.codec_name.clone(),
                media_config.clock_rate,
                None,
            ))
            .map_err(|e| {
                Error::SdpCreationError(format!("Failed to add RTPMap attribute: {e}"))
            })?;

        for candidate in ice_agent.get_local_candidates() {
            media_description
                .add_attribute(Attribute::Candidate(candidate.clone()))
                .map_err(|e| {
                    Error::SdpCreationError(format!("Failed to add Candidate attribute: {e}"))
                })?;
        }

        let mut sdp = SessionDescriptionProtocol::new(vec![media_description], sdp_config);

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
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::config::{MediaConfig, IceConfig, SdpConfig};
    use std::net::UdpSocket;

    fn get_configs() -> (MediaConfig, IceConfig, SdpConfig) {
        let media = MediaConfig {
            camera_index: 0, frame_width: 640.0, frame_height: 480.0, frame_rate: 30,
            h264_idr_interval: 30, rtp_max_chunk_size: 1200, default_ssrc: 1234,
            rtp_payload_type: 96, codec_name: "H264".into(), clock_rate: 90000,
            rtp_version: 2, media_type: "video".into(), media_protocol: "RTP/AVP".into()
        };
        let ice = IceConfig {
            foundation: "1".into(), transport: "UDP".into(), component_id: 1,
            host_priority_preference: 126, srflx_priority_preference: 100, host_local_preference: 65535
        };
        let sdp = SdpConfig {
            version: 0, origin_id: 123, session_name: "Test".into(), timing: "0 0".into(),
            connection_data_net_type: "IN".into(), connection_data_addr_type: "IP4".into(), connection_data_address: "0.0.0.0".into()
        };
        (media, ice, sdp)
    }

    #[test]
    fn test_full_negotiation_cycle() {
        let (media, ice, sdp_conf) = get_configs();

        let socket_alice = UdpSocket::bind("0.0.0.0:0").unwrap();
        let socket_bob = UdpSocket::bind("0.0.0.0:0").unwrap();

        let mut alice = Client::new(socket_alice, &media, &ice, &sdp_conf).unwrap();
        let mut bob = Client::new(socket_bob, &media, &ice, &sdp_conf).unwrap();

        let offer = alice.get_offer();
        assert!(!offer.is_empty(), "La oferta no debería estar vacía");
        assert!(offer.contains("a=candidate"), "La oferta debe tener candidatos ICE");

        let answer = bob.process_offer(&offer).expect("Bob falló al procesar la oferta");
        assert!(!answer.is_empty(), "La respuesta no debería estar vacía");

        let result = alice.process_answer(&answer);
        assert!(result.is_ok(), "Alice debería aceptar la respuesta de Bob");
    }
}