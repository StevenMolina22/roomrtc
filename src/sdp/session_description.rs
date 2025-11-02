use super::Attribute;
use super::MediaDescription;
use super::error::SdpError as Error;

use std::collections::HashSet;
use std::fmt::Display;
use std::str::FromStr;

/// SDP keys used when parsing session description lines.
const MEDIA_DESCRIPTION_KEY: &str = "m";
const ATTRIBUTE_KEY: &str = "a";

/// In-memory representation of a SDP session description.
///
/// This struct holds a reduced subset of SDP fields needed by the
/// project: version, origin id, session name, timing, a list of media
/// descriptions and connection data. Only `media_descriptions` is
/// publicly exposed; other fields are initialized with defaults when
/// parsing.
pub struct SessionDescriptionProtocol {
    /// SDP version (`v=`). Defaults to 0 in created instances.
    version: u8,

    /// Origin/session id. Stored as `usize`.
    origin_id: usize,

    /// Session name (`s=`) - default `-`.
    session_name: String,

    /// Timing (`t=`) string, default `"0 0"`.
    timing: String,

    /// Media descriptions (`m=` sections) parsed from the SDP.
    pub media_descriptions: Vec<MediaDescription>,

    /// Connection data (`c=`), e.g. `IN IP4 0.0.0.0` by default.
    connection_data: String,
}

impl FromStr for SessionDescriptionProtocol {
    type Err = Error;

    /// Parse a full SDP session description from a string.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut media_descriptions = Vec::new();

        for line in s.split('\n') {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                return Err(Error::InvalidMediaDescriptionFormatError);
            };

            match key {
                MEDIA_DESCRIPTION_KEY => {
                    // parse an `m=` media description line and append it
                    // to the list of media descriptions
                    handle_media_description_line(value, &mut media_descriptions)?;
                }
                ATTRIBUTE_KEY => {
                    // parse an `a=` attribute line and attach it to the
                    // last media description
                    handle_attribute_line(value, &mut media_descriptions)?;
                }
                _ => {}
            }
        }
        Ok(Self::new(media_descriptions))
    }
}

impl Display for SessionDescriptionProtocol {
    /// Render the full SDP session description as a string.
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
    /// Create a new `SessionDescriptionProtocol` with the given media
    /// descriptions. Other fields are initialized with default values.
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
    /// Create an SDP answer based on this local description and a remote
    /// offer description (`offer_sdp`).
    ///
    /// The function compares local media descriptions with the offer and
    /// builds answer media descriptions that preserve compatible
    /// formats/attributes. It returns a new `SessionDescriptionProtocol`
    /// containing only the compatible media sections.
    pub fn create_answer(&self, offer_sdp: &Self) -> Result<Self, Error> {
        let mut answer_media_descriptions = Vec::new();

        for local_md in &self.media_descriptions {
            if let Some(offer_md) = offer_sdp
                .media_descriptions
                .iter()
                .find(|m| m.media_type == local_md.media_type && m.protocol == local_md.protocol)
            {
                let (answer_md_fmts, answer_md_attributes) =
                    compatible_attributes_data(local_md, offer_md);
                let answer_md = create_answer_md(local_md, answer_md_fmts, answer_md_attributes)?;
                answer_media_descriptions.push(answer_md);
            }
        }

        Ok(Self::new(answer_media_descriptions))
    }

    /// Set the `c=` connection data for the session description.
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
) -> Result<MediaDescription, Error> {
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
        answer_md.add_attribute(attr)?;
    }

    Ok(answer_md)
}

fn handle_media_description_line(
    line: &str,
    media_descriptions: &mut Vec<MediaDescription>,
) -> Result<(), Error> {
    // Parse the media description `m=` line and append to the list.
    media_descriptions.push(MediaDescription::from_str(line)?);
    Ok(())
}

fn handle_attribute_line(
    line: &str,
    media_descriptions: &mut [MediaDescription],
) -> Result<(), Error> {
    let attribute = Attribute::from_str(line)?;
    // Attach the parsed attribute to the last media description in the list.
    match media_descriptions.last_mut() {
        Some(m) => m.add_attribute(attribute)?,
        None => return Err(Error::MissingMediaDescriptionError),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdp::error::SdpError as Error;

    fn make_test_media_description() -> Result<MediaDescription, Error> {
        let mut fmts = HashSet::new();
        fmts.insert(96);

        let mut md = MediaDescription::new("audio".into(), 5004, "RTP/AVP".into(), fmts);
        md.add_attribute(Attribute::RTPMap(96, "opus".into(), 48000, Some("2".into())))?;
        Ok(md)
    }

    #[test]
    fn test_from_str_generates_valid_session_description() -> Result<(), Error> {
        let sdp_line = "\
            v=0
            o=0
            s=-
            t=0 0
            c=IN IP4 127.0.0.1
            m=audio 5004 RTP/AVP 96
            a=rtpmap:96 opus/48000/2";

        let sdp = SessionDescriptionProtocol::from_str(sdp_line)?;
        assert_eq!(sdp.version, 0);
        assert_eq!(sdp.origin_id, 0);
        assert_eq!(sdp.session_name, "-");
        assert_eq!(sdp.timing, "0 0");
        assert_eq!(sdp.connection_data, "IN IP4 0.0.0.0");
        assert_eq!(sdp.media_descriptions.len(), 1);

        let md = sdp.media_descriptions.first().ok_or(Error::MissingMediaDescriptionError)?;
        assert_eq!(md.media_type, "audio");
        assert_eq!(md.protocol, "RTP/AVP");
        assert_eq!(md.port, 5004);
        assert!(md.attributes.iter().any(|a| matches!(a, Attribute::RTPMap(_, _, _, _))));
        Ok(())
    }

    #[test]
    fn test_from_str_with_invalid_sdp_line() {
        let sdp_line = "\
            v=0
            o=0
            s=-
            t=0 0
            c IN IP4 127.0.0.1
            m=audio 5004 RTP/AVP 96
            a=rtpmap:96 opus/48000/2";

        let sdp = SessionDescriptionProtocol::from_str(sdp_line);
        assert!(sdp.is_err());
        if let Err(err) = sdp {
            assert!(matches!(err, Error::InvalidMediaDescriptionFormatError));
        } else {
            panic!("Expected InvalidMediaDescriptionFormatError");
        }
    }

    #[test]
    fn test_display_formats_sdp_correctly() -> Result<(), Error> {
        let md = make_test_media_description()?;
        let sdp = SessionDescriptionProtocol::new(vec![md]);

        let out = format!("{}", sdp);
        assert!(out.contains("v=0"));
        assert!(out.contains("m=audio"));
        assert!(out.contains("a=rtpmap:96 opus/48000/2"));
        Ok(())
    }

    #[test]
    fn test_set_connection_data_updates_value() -> Result<(), Error> {
        let md = make_test_media_description()?;
        let mut sdp = SessionDescriptionProtocol::new(vec![md]);

        assert_eq!(sdp.connection_data, "IN IP4 0.0.0.0");

        sdp.set_connection_data("IN", "IP4", "127.0.0.1");
        assert_eq!(sdp.connection_data, "IN IP4 127.0.0.1");
        Ok(())
    }

    #[test]
    fn test_set_connection_data_changes_value() {
        let mut sdp = SessionDescriptionProtocol::new(vec![]);
        sdp.set_connection_data("IN", "IP4", "192.168.0.5");

        assert_eq!(sdp.connection_data, "IN IP4 192.168.0.5");
    }

    #[test]
    fn test_create_answer_filters_incompatible_formats() -> Result<(), Error> {
        let local_md = make_test_media_description()?;
        let local_sdp = SessionDescriptionProtocol::new(vec![local_md]);

        let mut remote_fmts = HashSet::new();
        remote_fmts.insert(97);
        let mut remote_md = MediaDescription::new("audio".into(), 6000, "RTP/AVP".into(), remote_fmts);
        remote_md.add_attribute(Attribute::RTPMap(97, "vp8".into(), 90000, Some("1".into())))?;

        let remote_sdp = SessionDescriptionProtocol::new(vec![remote_md]);

        let answer = local_sdp.create_answer(&remote_sdp)?;
        assert_eq!(answer.media_descriptions.len(), 1);

        let answer_md = &answer.media_descriptions[0];
        assert!(answer_md.fmts.is_empty());
        Ok(())
    }

    #[test]
    fn test_create_answer_preserves_compatible_formats() -> Result<(), Error> {
        let local_md = make_test_media_description()?;
        let local_sdp = SessionDescriptionProtocol::new(vec![local_md]);

        let mut offer_md = MediaDescription::new(
            "audio".into(),
            6000,
            "RTP/AVP".into(),
            HashSet::from([96]),
        );
        offer_md
            .add_attribute(Attribute::RTPMap(96, "opus".into(), 48000, Some("2".into())))?;

        let offer_sdp = SessionDescriptionProtocol::new(vec![offer_md]);

        let answer = local_sdp.create_answer(&offer_sdp)?;
        assert_eq!(answer.media_descriptions.len(), 1);

        let answer_md = &answer.media_descriptions[0];
        assert!(answer_md.fmts.contains(&96));
        assert!(answer_md.attributes.iter().any(|a| matches!(a, Attribute::RTPMap(_, _, _, _))));
        Ok(())
    }
}
