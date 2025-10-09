use crate::media_description::MediaDescription;
use std::fmt::Display;
use std::str::FromStr;

const MEDIA_DESCRIPTION_KEY: &str = "m";
const ATTRIBUTE_KEY: &str = "a";

pub struct SessionDescriptionProtocol {
    version: u8,
    origin_id: usize,
    session_name: String,
    timing: String,
    media_descriptions: Vec<MediaDescription>,
    connection_data: String,
}

impl FromStr for SessionDescriptionProtocol {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut media_descriptions = Vec::new();

        for line in s.split('\n') {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = match line.split_once('=') {
                Some((k, v)) => (k, v),
                None => return Err(()),
            };

            match key {
                MEDIA_DESCRIPTION_KEY => {
                    handle_media_description_line(value, &mut media_descriptions)?;
                }
                ATTRIBUTE_KEY => {
                    handle_attribute_line(value, &mut media_descriptions)?;
                }
                _ => continue,
            }
        }
        Ok(Self::new(media_descriptions))
    }
}

impl Display for SessionDescriptionProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut content = format!(
            "v={}\no={}\ns={}\nt={}\nc={}",
            self.version, self.origin_id, self.session_name, self.timing, self.connection_data
        );

        for media_description in &self.media_descriptions {
            content = format!("{}\n{}", content, media_description);
        }

        write!(f, "{}", content)
    }
}

impl SessionDescriptionProtocol {
    pub fn new(media_descriptions: Vec<MediaDescription>) -> Self {
        Self {
            version: 0,
            origin_id: 0,
            session_name: "-".into(),
            timing: "0 0".into(),
            media_descriptions,
            connection_data: "IN IP4 0.0.0.0".into(),
        }
    }

    pub fn create_answer(&self, _offer_sdp: SessionDescriptionProtocol) -> Self {
        // por cada md de self:
        //     busque md compatible en offer_sdp, si encuentra:
        //         busca a compatible, si encuentra:
        //             agrega md a respuesta compatible si no lo agrego antes
        //             agrega attr compatible a md de respuesta

        SessionDescriptionProtocol::new(Vec::new())
    }

    pub fn set_connection_data(&mut self, net_type: String, addr_type: String, address: String) {
        self.connection_data = format!("{} {} {}", net_type, addr_type, address);
    }
}

fn handle_media_description_line(
    line: &str,
    media_descriptions: &mut Vec<MediaDescription>,
) -> Result<(), ()> {
    media_descriptions.push(MediaDescription::from_str(line)?);
    Ok(())
}

fn handle_attribute_line(
    line: &str,
    media_descriptions: &mut Vec<MediaDescription>,
) -> Result<(), ()> {
    let (attribute_type, attribute_body) = if line.starts_with("candidate:") {
        // Special handling for candidate lines: "candidate:1 1 UDP ..." -> ("candidate", "1 1 UDP ...")
        ("candidate", &line[10..]) // Skip "candidate:"
    } else {
        // Normal attribute parsing: "rtpmap 111 OPUS/48000/2" -> ("rtpmap", "111 OPUS/48000/2")
        line.split_once(' ').unwrap()
    };

    match media_descriptions.last_mut() {
        Some(m) => m
            .add_attribute(attribute_type.into(), attribute_body.into())
            .map_err(|_| ())?,
        None => return Err(()),
    }
    Ok(())
}
