use crate::sdp::Attribute;
use crate::sdp::MediaDescription;

use std::collections::HashSet;
use std::fmt::Display;
use std::str::FromStr;

const MEDIA_DESCRIPTION_KEY: &str = "m";
const ATTRIBUTE_KEY: &str = "a";

pub struct SessionDescriptionProtocol {
    version: u8,
    origin_id: usize,
    session_name: String,
    timing: String,
    pub media_descriptions: Vec<MediaDescription>,
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

            let Some((key, value)) = line.split_once('=') else {
                return Err(());
            };

            match key {
                MEDIA_DESCRIPTION_KEY => {
                    handle_media_description_line(value, &mut media_descriptions)?;
                }
                ATTRIBUTE_KEY => {
                    handle_attribute_line(value, &mut media_descriptions)?;
                }
                _ => {}
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
            content = format!("{content}\n{media_description}");
        }

        write!(f, "{content}",)
    }
}

impl SessionDescriptionProtocol {
    #[must_use]
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

    pub fn create_answer(&self, offer_sdp: &Self) -> Result<Self, ()> {
        let mut answer_media_descriptions = Vec::new();

        for local_md in &self.media_descriptions {
            if let Some(offer_md) = offer_sdp
                .media_descriptions
                .iter()
                .find(|m| m.media_type == local_md.media_type && m.protocol == local_md.protocol)
            {
                let (answer_md_fmts, answer_md_attributes) =
                    compatible_attributes_data(local_md, offer_md);
                let answer_md = create_answer_md(local_md, answer_md_fmts, answer_md_attributes)
                    .map_err(|()| ())?;
                answer_media_descriptions.push(answer_md);
            }
        }

        Ok(Self::new(answer_media_descriptions))
    }

    pub fn set_connection_data(&mut self, net_type: &str, addr_type: &str, address: &str) {
        self.connection_data = format!("{net_type} {addr_type} {address}");
    }
}

// TO DO: Review if all local attributes should be copied into the sdp answer (ice candidates, fingerprints,...)
fn compatible_attributes_data(
    local_md: &MediaDescription,
    offer_md: &MediaDescription,
) -> (HashSet<u8>, Vec<Attribute>) {
    let mut answer_md_attributes = Vec::new();
    let mut answer_md_fmts = HashSet::new();

    for local_attr in &local_md.attributes {
        if let Attribute::RTPMap(local_fmt, local_encoding_name, _, _) = &local_attr {
            for offer_attr in &offer_md.attributes {
                if let Attribute::RTPMap(_, offer_encoding_name, _, _) = &offer_attr
                    && local_encoding_name == offer_encoding_name
                {
                    answer_md_fmts.insert(*local_fmt);
                    answer_md_attributes.push(local_attr.clone());
                }
            }
        } else {
            answer_md_attributes.push(local_attr.clone());
        }
    }

    (answer_md_fmts, answer_md_attributes)
}

fn create_answer_md(
    local_md: &MediaDescription,
    answer_md_fmts: HashSet<u8>,
    answer_md_attributes: Vec<Attribute>,
) -> Result<MediaDescription, ()> {
    let mut answer_md = MediaDescription::new(
        local_md.media_type.clone(),
        if answer_md_attributes.is_empty() {
            0
        } else {
            local_md.port
        },
        local_md.protocol.clone(),
        answer_md_fmts,
    );

    for attr in answer_md_attributes {
        answer_md.add_attribute(attr).map_err(|_| ())?;
    }

    Ok(answer_md)
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
    media_descriptions: &mut [MediaDescription],
) -> Result<(), ()> {
    let attribute = Attribute::from_str(line)?;

    match media_descriptions.last_mut() {
        Some(m) => m.add_attribute(attribute).map_err(|_| ())?,
        None => return Err(()),
    }
    Ok(())
}
