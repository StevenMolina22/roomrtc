use super::error::ClientError as Error;
use crate::{
    ice::IceAgent,
    sdp::{Attribute, MediaDescription, SessionDescriptionProtocol},
};
use std::collections::HashSet;
use std::io::{BufRead, Write};
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

        Self { sdp, ice_agent }
    }

    pub fn offer_sdp<R: BufRead, W: Write>(
        &mut self,
        mut in_buff: R,
        mut out_buff: W,
    ) -> Result<(), Error> {
        // Send our SDP offer
        out_buff.write_all(self.sdp.to_string().as_bytes()).unwrap();
        out_buff.write_all(b"\n").unwrap();
        out_buff.flush().unwrap();

        // Read the complete SDP answer
        let mut answer = String::new();
        loop {
            let mut line = String::new();
            if in_buff.read_line(&mut line).unwrap() == 0 {
                break;
            }
            answer.push_str(&line);
        }
        let sdp_answer = SessionDescriptionProtocol::from_str(&answer)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;
        eprintln!("Answer received");

        // Find and process the remote ICE candidate

        self.process_remote_sdp(&sdp_answer)
    }

    pub fn answer_sdp<R: BufRead, W: Write>(
        &mut self,
        mut in_buff: R,
        mut out_buff: W,
    ) -> Result<(), Error> {
        let mut offer_string = String::new();
        // Read the complete SDP offer
        loop {
            let mut line = String::new();
            let bytes_read = in_buff.read_line(&mut line).unwrap();
            if bytes_read == 0 {
                break;
            }
            offer_string.push_str(&line);
        }

        // Parse offer to make sure it's valid
        let sdp_offer = SessionDescriptionProtocol::from_str(&offer_string)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;
        eprintln!("Offer received");

        // Send our SDP answer
        let sdp_answer = self
            .sdp
            .create_answer(&sdp_offer)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;
        out_buff
            .write_all(sdp_answer.to_string().as_bytes())
            .unwrap();
        out_buff.write_all(b"\n").unwrap();
        out_buff.flush().unwrap();

        // Extract and process the remote ICE candidate from the offer
        self.process_remote_sdp(&sdp_offer)
    }

    pub fn process_offer(&mut self, offer_str: &str) -> Result<String, Error> {
        let sdp_offer = SessionDescriptionProtocol::from_str(offer_str)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        // Process remote candidates
        self.process_remote_sdp(&sdp_offer)?;

        // Create and return answer
        self.sdp
            .create_answer(&sdp_offer)
            .map(|answer_sdp| answer_sdp.to_string())
            .map_err(|e| Error::SdpCreationError(e.to_string()))
    }
    pub fn process_answer(&mut self, answer_str: &str) -> Result<(), Error> {
        let sdp_answer = SessionDescriptionProtocol::from_str(answer_str)
            .map_err(|e| Error::SdpCreationError(e.to_string()))?;

        // Process remote candidates
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
