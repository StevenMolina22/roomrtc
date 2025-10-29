use super::error::ClientError as Error;
use crate::{
    ice::IceAgent,
    sdp::{Attribute, MediaDescription, SessionDescriptionProtocol},
};
use std::collections::HashSet;
use std::str::FromStr;

const MEDIA_TYPE: &str = "video";
const MEDIA_PORT: u16 = 4000;
const MEDIA_PROTOCOL: &str = "RTP/AVP";
const MEDIA_FMT: u8 = 111;

pub struct Client {
    pub sdp: SessionDescriptionProtocol,
    pub ice_agent: IceAgent,
}


impl Client {
    pub fn new() -> Self {
        let mut ice_agent = IceAgent::new();
        if ice_agent.gather_candidates(MEDIA_PORT).is_err() {
            panic!("Failed to gather ICE candidates");
        }

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

    pub fn process_offer(&mut self, offer_str: &str) -> Result<String, Error> {
        let sdp_offer = SessionDescriptionProtocol::from_str(offer_str)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        let answer = self.sdp
            .create_answer(&sdp_offer)
            .map(|answer_sdp| answer_sdp.to_string())
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        self.process_remote_sdp(&sdp_offer)?;

        Ok(answer)
    }
    pub fn process_answer(&mut self, answer_str: &str) -> Result<(), Error> {
        let sdp_answer = SessionDescriptionProtocol::from_str(answer_str)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        self.process_remote_sdp(&sdp_answer)?;

        Ok(())
    }

    pub fn get_offer(&self) -> String {
        self.sdp.to_string()
    }

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
