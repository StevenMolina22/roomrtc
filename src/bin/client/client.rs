use std::{io::{BufRead, Write}, str::FromStr};

use roomrtc::{
    sdp::SessionDescriptionProtocol,
    media_description::MediaDescription,
};

const MEDIA_TYPE: &str = "video";
const MEDIA_PORT: u16 = 4000;
const MEDIA_PROTOCOL: &str = "RTP/AVP";
const MEDIA_FMT: usize = 111;

pub struct Client {
    sdp: SessionDescriptionProtocol,
}

impl Client {
    pub fn new() -> Self {
        let mut media_description = MediaDescription::new(MEDIA_TYPE.into(), MEDIA_PORT, MEDIA_PROTOCOL.into(), vec![MEDIA_FMT]);
        media_description.add_attribute("rtpmap".into(), "111 OPUS/48000/2".into()).map_err(|_| ()).unwrap();

        Self {
            sdp: SessionDescriptionProtocol::new(vec![media_description]),
        }
    }

    pub fn offer_sdp<R: BufRead, W: Write>(&self, mut in_buff: R, mut out_buff: W) -> Result<(), ()>{
        out_buff.write_all(self.sdp.to_string().as_bytes()).unwrap();
        out_buff.write_all("\n".as_bytes()).unwrap();
        out_buff.flush().unwrap();

        let mut answer = String::new();
        in_buff.read_line(&mut answer).unwrap();

        let _ = SessionDescriptionProtocol::from_str(&answer).map_err(|_| ())?;

        println!("Answer received");
        Ok(())
    }

    pub fn answer_sdp<R: BufRead, W: Write>(&self, mut in_buff: R, mut out_buff: W) -> Result<(), ()> {
        let mut offer = String::new();
        in_buff.read_line(&mut offer).unwrap();

        let offer_sdp = SessionDescriptionProtocol::from_str(&offer).map_err(|_| ())?;
        println!("Offer received");

        let sdp_answer = self.sdp.create_answer(offer_sdp);

        out_buff.write_all(sdp_answer.to_string().as_bytes()).unwrap();
        out_buff.flush().unwrap();
        Ok(())
    }
}