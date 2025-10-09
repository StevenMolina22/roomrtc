use std::{
    io::{BufRead, Write},
    str::FromStr,
};

use roomrtc::{
    ice::IceAgent, media_description::MediaDescription, sdp::SessionDescriptionProtocol,
};

const MEDIA_TYPE: &str = "video";
const MEDIA_PORT: u16 = 4000;
const MEDIA_PROTOCOL: &str = "RTP/AVP";
const MEDIA_FMT: usize = 111;

pub struct Client {
    sdp: SessionDescriptionProtocol,
    ice_agent: IceAgent,
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
            vec![MEDIA_FMT],
        );
        media_description
            .add_attribute("rtpmap".into(), "111 OPUS/48000/2".into())
            .unwrap();

        // Add our ICE candidate to the media description
        if let Some(candidate_line) = ice_agent.generate_candidate_lines().first() {
            // Split "a=candidate:" into type and value parts
            if let Some((attr_type, attr_body)) = candidate_line.trim().split_once(':') {
                media_description
                    .add_attribute(
                        attr_type.strip_prefix("a=").unwrap().into(),
                        attr_body.into(),
                    )
                    .unwrap();
            }
        }

        // Create SDP with our media description
        let mut sdp = SessionDescriptionProtocol::new(vec![media_description]);

        // Set connection info using our local IP
        if let Some(local_candidate) = ice_agent.get_local_candidate() {
            sdp.set_connection_data("IN".into(), "IP4".into(), local_candidate.address.clone());
        }

        Self { sdp, ice_agent }
    }

    pub fn offer_sdp<R: BufRead, W: Write>(
        &mut self,
        mut in_buff: R,
        mut out_buff: W,
    ) -> Result<(), ()> {
        // Send our SDP offer
        out_buff.write_all(self.sdp.to_string().as_bytes()).unwrap();
        out_buff.write_all("\n".as_bytes()).unwrap();
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
        let _answer_sdp = SessionDescriptionProtocol::from_str(&answer).map_err(|_| ())?;
        eprintln!("Answer received");

        // Find and process the remote ICE candidate
        if let Some(line) = answer.lines().find(|l| l.starts_with("a=candidate:")) {
            if let Ok(remote_candidate) = IceAgent::parse_candidate_line(line) {
                self.ice_agent
                    .add_remote_candidate(remote_candidate)
                    .unwrap();
                self.ice_agent.start_connectivity_checks().unwrap();
            } else {
                eprintln!("Failed to parse remote candidate line");
            }
        } else {
            eprintln!("No ICE candidate found in the answer");
        }

        Ok(())
    }

    pub fn answer_sdp<R: BufRead, W: Write>(
        &mut self,
        mut in_buff: R,
        mut out_buff: W,
    ) -> Result<(), ()> {
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
        let _offer_sdp = SessionDescriptionProtocol::from_str(&offer_string).map_err(|_| ())?;
        eprintln!("Offer received");

        // Send our SDP answer
        let sdp_answer = self.sdp.to_string();
        out_buff.write_all(sdp_answer.as_bytes()).unwrap();
        out_buff.write_all("\n".as_bytes()).unwrap();
        out_buff.flush().unwrap();

        // Extract and process the remote ICE candidate from the offer
        if let Some(line) = offer_string.lines().find(|l| l.starts_with("a=candidate:"))
            && let Ok(remote_candidate) = IceAgent::parse_candidate_line(line) {
                self.ice_agent
                    .add_remote_candidate(remote_candidate)
                    .unwrap();
                self.ice_agent.start_connectivity_checks().unwrap();
            }

        Ok(())
    }
}
