use super::Attribute;
use super::MediaDescription;
use super::error::SdpError as Error;
use crate::config::SdpConfig;

use std::collections::HashSet;
use std::fmt::Display;
use std::str::FromStr;

/// SDP keys used when parsing session description lines.
///
/// These constants represent the single-letter SDP line prefixes used
/// when processing a textual SDP. They are kept here to avoid magic
/// strings scattered through the parser.
///
/// - `m` indicates the start of a media description line ("m=").
/// - `a` indicates an attribute line ("a=") that should be attached
///   to the most-recently-parsed media description.
const MEDIA_DESCRIPTION_KEY: &str = "m";
const ATTRIBUTE_KEY: &str = "a";

/// In-memory representation of an SDP session description.
///
/// The parser stores a subset of the SDP fields required by
/// the project: version (v=), origin id (o=), session name (s=),
/// timing (t=), a list of media descriptions (m= sections) and the
/// session-level connection data (c=). Only `media_descriptions` is
/// publicly exposed; other fields are initialized with sensible
/// defaults when creating or parsing an SDP.
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
    ///
    /// Each entry corresponds to one media section in the textual SDP
    /// and contains its attributes (including `a=rtpmap` and `a=candidate`).
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
        Ok(Self::new(media_descriptions, &SdpConfig::default()))
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
    /// descriptions and SDP configuration values.
    #[must_use]
    pub fn new(media_descriptions: Vec<MediaDescription>, sdp_config: &SdpConfig) -> Self {
        Self {
            version: sdp_config.version,
            origin_id: sdp_config.origin_id,
            session_name: sdp_config.session_name.clone(),
            timing: sdp_config.timing.clone(),
            media_descriptions,
            connection_data: format!(
                "{} {} {}",
                sdp_config.connection_data_net_type,
                sdp_config.connection_data_addr_type,
                sdp_config.connection_data_address
            ),
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

        Ok(Self::new(
            answer_media_descriptions,
            &SdpConfig {
                version: self.version,
                origin_id: self.origin_id,
                session_name: self.session_name.clone(),
                timing: self.timing.clone(),
                connection_data_net_type: "IN".to_string(),
                connection_data_addr_type: "IP4".to_string(),
                connection_data_address: "0.0.0.0".to_string(),
            },
        ))
    }

    /// Set the `c=` connection data for the session description.
    pub fn set_connection_data(&mut self, net_type: &str, addr_type: &str, address: &str) {
        self.connection_data = format!("{net_type} {addr_type} {address}");
    }
}

/// Given a local media description and an offer media description,
/// compute the set of formats (payload types) and the list of
/// attributes that should be included in the answer.
///
/// The returned tuple contains:
/// - a `HashSet<u8>` with the compatible payload type numbers
/// - a `Vec<Attribute>` with attributes copied from the local media
///   description when appropriate
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

/// Build a `MediaDescription` for the answer from the given local
/// media description, the set of selected formats and the attributes to
/// include.
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

/// Parse a single `m=` media description line and append the
/// resulting `MediaDescription` to the provided vector.
fn handle_media_description_line(
    line: &str,
    media_descriptions: &mut Vec<MediaDescription>,
) -> Result<(), Error> {
    // Parse the media description `m=` line and append to the list.
    media_descriptions.push(MediaDescription::from_str(line)?);
    Ok(())
}

/// Parse an `a=` attribute line and attach it to the last media
/// description in the provided slice. Returns an error if there is no
/// previous media description to attach to.
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

    fn make_test_sdp_config() -> SdpConfig {
        SdpConfig {
            version: 0,
            origin_id: 0,
            session_name: "-".to_string(),
            timing: "0 0".to_string(),
            connection_data_net_type: "IN".to_string(),
            connection_data_addr_type: "IP4".to_string(),
            connection_data_address: "0.0.0.0".to_string(),
        }
    }

    fn make_test_media_description() -> Result<MediaDescription, Error> {
        let mut fmts = HashSet::new();
        fmts.insert(96);

        let mut md = MediaDescription::new("audio".into(), 5004, "RTP/AVP".into(), fmts);
        md.add_attribute(Attribute::RTPMap(
            96,
            "opus".into(),
            48000,
            Some("2".into()),
        ))?;
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

        let md = sdp
            .media_descriptions
            .first()
            .ok_or(Error::MissingMediaDescriptionError)?;
        assert_eq!(md.media_type, "audio");
        assert_eq!(md.protocol, "RTP/AVP");
        assert_eq!(md.port, 5004);
        assert!(
            md.attributes
                .iter()
                .any(|a| matches!(a, Attribute::RTPMap(_, _, _, _)))
        );
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
        let sdp = SessionDescriptionProtocol::new(vec![md], &make_test_sdp_config());

        let out = format!("{sdp}");
        assert!(out.contains("v=0"));
        assert!(out.contains("m=audio"));
        assert!(out.contains("a=rtpmap:96 opus/48000/2"));
        Ok(())
    }

    #[test]
    fn test_set_connection_data_updates_value() -> Result<(), Error> {
        let md = make_test_media_description()?;
        let mut sdp = SessionDescriptionProtocol::new(vec![md], &make_test_sdp_config());

        assert_eq!(sdp.connection_data, "IN IP4 0.0.0.0");

        sdp.set_connection_data("IN", "IP4", "127.0.0.1");
        assert_eq!(sdp.connection_data, "IN IP4 127.0.0.1");
        Ok(())
    }

    #[test]
    fn test_set_connection_data_changes_value() {
        let mut sdp = SessionDescriptionProtocol::new(vec![], &make_test_sdp_config());
        sdp.set_connection_data("IN", "IP4", "192.168.0.5");

        assert_eq!(sdp.connection_data, "IN IP4 192.168.0.5");
    }

    #[test]
    fn test_create_answer_filters_incompatible_formats() -> Result<(), Error> {
        let local_md = make_test_media_description()?;
        let local_sdp = SessionDescriptionProtocol::new(vec![local_md], &make_test_sdp_config());

        let mut remote_fmts = HashSet::new();
        remote_fmts.insert(97);
        let mut remote_md =
            MediaDescription::new("audio".into(), 6000, "RTP/AVP".into(), remote_fmts);
        remote_md.add_attribute(Attribute::RTPMap(97, "vp8".into(), 90000, Some("1".into())))?;

        let remote_sdp = SessionDescriptionProtocol::new(vec![remote_md], &make_test_sdp_config());

        let answer = local_sdp.create_answer(&remote_sdp)?;
        assert_eq!(answer.media_descriptions.len(), 1);

        let answer_md = &answer.media_descriptions[0];
        assert!(answer_md.fmts.is_empty());
        Ok(())
    }

    #[test]
    fn test_create_answer_preserves_compatible_formats() -> Result<(), Error> {
        let local_md = make_test_media_description()?;
        let local_sdp = SessionDescriptionProtocol::new(vec![local_md], &make_test_sdp_config());

        let mut offer_md =
            MediaDescription::new("audio".into(), 6000, "RTP/AVP".into(), HashSet::from([96]));
        offer_md.add_attribute(Attribute::RTPMap(
            96,
            "opus".into(),
            48000,
            Some("2".into()),
        ))?;

        let offer_sdp = SessionDescriptionProtocol::new(vec![offer_md], &make_test_sdp_config());

        let answer = local_sdp.create_answer(&offer_sdp)?;
        assert_eq!(answer.media_descriptions.len(), 1);

        let answer_md = &answer.media_descriptions[0];
        assert!(answer_md.fmts.contains(&96));
        assert!(
            answer_md
                .attributes
                .iter()
                .any(|a| matches!(a, Attribute::RTPMap(_, _, _, _)))
        );
        Ok(())
    }
}
