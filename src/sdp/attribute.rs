use super::error::SdpError as Error;
use crate::ice::Candidate;
use crate::ice::CandidateType;
use std::fmt;
use std::str::FromStr;

/// SDP attribute keys used by the parser.
const CANDIDATE_ATTR_KEY: &str = "candidate";
const RTPMAP_ATTR_KEY: &str = "rtpmap";

/// SDP attribute representation used by this module.
///
/// The `Attribute` enum models a subset of SDP `a=` attributes that the
/// project needs to parse and emit: ICE candidates (`candidate`) and
/// payload mapping (`rtpmap`). Each variant contains the parsed data for
/// that attribute.
#[derive(Clone)]
pub enum Attribute {
    /// An ICE candidate attribute containing a full `Candidate`.
    Candidate(Candidate),
    // (rtp_payload_type, rtp_codec_name, rtp_clock_rate, rtp_encoding_params)
    RTPMap(u8, String, u32, Option<String>),
}

impl fmt::Display for Attribute {
    /// Render the attribute back to SDP attribute line form.
    ///
    /// - Candidate: `a=candidate:<foundation> <component> <transport> <priority> <address> <port> typ <type>`
    /// - RTPMap: `a=rtpmap:<fmt> <encoding_name>/<clock_rate>[/<encoding_params>]`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Candidate(candidate) => {
                write!(
                    f,
                    "a=candidate:{} {} {} {} {} {} typ {}",
                    candidate.foundation,
                    candidate.component_id,
                    candidate.transport,
                    candidate.priority,
                    candidate.address,
                    candidate.port,
                    candidate.candidate_type,
                )
            }

            Self::RTPMap(fmt, enc_name, clock_rate, encoding_params) => {
                if let Some(params) = encoding_params {
                    write!(f, "a=rtpmap:{fmt} {enc_name}/{clock_rate}/{params}")
                } else {
                    write!(f, "a=rtpmap:{fmt} {enc_name}/{clock_rate}")
                }
            }
        }
    }
}

impl FromStr for Attribute {
    type Err = Error;

    /// Parse an SDP attribute line (without the leading `a=`) into an
    /// `Attribute`.
    ///
    /// The expected forms are `candidate:...` and `rtpmap:...`. Unknown
    /// or malformed inputs produce an appropriate `SdpError`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once(':') {
            Some((key, value)) if key == CANDIDATE_ATTR_KEY => parse_candidate_attr_values(value),
            Some((key, value)) if key == RTPMAP_ATTR_KEY => parse_rptmap_attr_values(value),
            _ => Err(Error::InvalidRtpMapFormatError),
        }
    }
}

/// Parse an `rtpmap` attribute value part into `Attribute::RTPMap`.
///
/// Expects a form like: `"96 opus/48000/2"`.
fn parse_rptmap_attr_values(values: &str) -> Result<Attribute, Error> {
    let parts = values.split_whitespace().collect::<Vec<&str>>();
    if parts.len() < 2 {
        return Err(Error::InvalidRtpMapFormatError);
    }

    let fmt = parts[0]
        .parse::<u8>()
        .map_err(|_| Error::InvalidRtpMapFormatError)?;

    let mut parts = parts[1].split('/');
    let encoding_name = parts.next().ok_or(Error::MissingEncodingNameError)?;
    if encoding_name.is_empty() {
        return Err(Error::MissingEncodingNameError);
    }
    let clock_rate = parts
        .next()
        .ok_or(Error::MissingClockRateError)?
        .parse::<u32>()
        .map_err(|_| Error::InvalidClockRateParsingError)?;
    let encoding_params = parts.next().map(std::string::ToString::to_string);

    if parts.next().is_some() {
        return Err(Error::ExtraRtpFieldsError);
    }

    Ok(Attribute::RTPMap(
        fmt,
        encoding_name.into(),
        clock_rate,
        encoding_params,
    ))
}

/// Parse a `candidate` attribute value into `Attribute::Candidate`.
///
/// The expected form follows the ICE candidate attribute grammar used in
/// SDP: `<foundation> <component> <transport> <priority> <address> <port> typ <type>`.
fn parse_candidate_attr_values(values: &str) -> Result<Attribute, Error> {
    let parts = values.split_whitespace().collect::<Vec<&str>>();
    if parts.len() < 8 {
        return Err(Error::InvalidLineFormatError);
    }

    if parts[6] != "typ" {
        return Err(Error::InvalidCandidateFormatError);
    }

    let transport = parts[2];

    Ok(Attribute::Candidate(Candidate::new(
        CandidateType::from_str(parts[7]).map_err(|_| Error::InvalidCandidateTypeError)?,
        parts[3]
            .parse::<u32>()
            .map_err(|_| Error::InvalidPriorityError)?,
        parts[4].to_string(),
        parts[5]
            .parse::<u16>()
            .map_err(|_| Error::InvalidPortError)?,
        parts[1]
            .parse::<u8>()
            .map_err(|_| Error::InvalidComponentIdError)?,
        parts[0].to_string(),
        transport.into(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ice::{Candidate, CandidateType};

    #[test]
    fn test_rtpmap_from_str_valid_basic() -> Result<(), Error> {
        let line = "rtpmap:96 opus/48000/2";
        let attr = Attribute::from_str(line)?;

        match attr {
            Attribute::RTPMap(fmt, enc, rate, params) => {
                assert_eq!(fmt, 96);
                assert_eq!(enc, "opus");
                assert_eq!(rate, 48000);
                assert_eq!(params, Some("2".to_string()));
            }
            _ => panic!("Expected Attribute::RTPMap"),
        }
        Ok(())
    }

    #[test]
    fn test_rtpmap_from_str_valid_no_params() -> Result<(), Error> {
        let line = "rtpmap:97 PCMU/8000";
        let attr = Attribute::from_str(line)?;

        match attr {
            Attribute::RTPMap(fmt, enc, rate, params) => {
                assert_eq!(fmt, 97);
                assert_eq!(enc, "PCMU");
                assert_eq!(rate, 8000);
                assert!(params.is_none());
            }
            _ => panic!("Expected Attribute::RTPMap"),
        }
        Ok(())
    }

    #[test]
    fn test_rtpmap_from_str_invalid_format() {
        let line = "rtpmap:";
        let attr = Attribute::from_str(line);

        assert!(matches!(attr, Err(Error::InvalidRtpMapFormatError)));
    }

    #[test]
    fn test_rtpmap_from_str_missing_encoding() {
        let line = "rtpmap:96 /48000/2";
        let attr = Attribute::from_str(line);

        assert!(matches!(attr, Err(Error::MissingEncodingNameError)));
    }

    #[test]
    fn test_rtpmap_from_str_missing_clock_rate() {
        let line = "rtpmap:96 opus";
        let attr = Attribute::from_str(line);

        assert!(matches!(attr, Err(Error::MissingClockRateError)));
    }

    #[test]
    fn test_invalid_rtpmap_extra_fields() {
        let line = "rtpmap:96 opus/48000/2/extra"; // Demasiados campos
        let result = Attribute::from_str(line);

        assert!(matches!(result, Err(Error::ExtraRtpFieldsError)));
    }

    #[test]
    fn test_display_rtpmap() {
        let attr = Attribute::RTPMap(96, "opus".into(), 48000, Some("2".into()));
        assert_eq!(attr.to_string(), "a=rtpmap:96 opus/48000/2");

        let attr_no_params = Attribute::RTPMap(97, "PCMU".into(), 8000, None);
        assert_eq!(attr_no_params.to_string(), "a=rtpmap:97 PCMU/8000");
    }

    #[test]
    fn test_candidate_from_str_valid() -> Result<(), Error> {
        let line = "candidate:1 1 udp 2122252543 192.168.1.5 54400 typ host";
        let attr = Attribute::from_str(line)?;

        match attr {
            Attribute::Candidate(cand) => {
                assert_eq!(cand.foundation, "1");
                assert_eq!(cand.component_id, 1);
                assert_eq!(cand.transport, "udp");
                assert_eq!(cand.priority, 2122252543);
                assert_eq!(cand.address, "192.168.1.5");
                assert_eq!(cand.port, 54400);
                assert!(matches!(cand.candidate_type, CandidateType::Host));
            }
            _ => panic!("Expected Attribute::Candidate"),
        }
        Ok(())
    }

    #[test]
    fn test_candidate_from_str_invalid_format() {
        let bad_lines = [
            "candidate:1 1 udp 2122252543 192.168.1.5 typ host",
            "candidate:1 1 udp 2122252543 192.168.1.5 54400",
            "candidate:1 1 udp x 192.168.1.5 54400 typ host",
            "candidate:1 1 udp 2122252543 192.168.1.5 x typ host",
        ];

        for line in bad_lines {
            assert!(Attribute::from_str(line).is_err(), "Should fail: {line}");
        }
    }

    #[test]
    fn test_candidate_from_str_invalid_format_missing_fields() {
        let candidate = "candidate:1 1 udp 2122252543 192.168.1.5 typ host";
        let attr = Attribute::from_str(candidate);

        assert!(matches!(attr, Err(Error::InvalidLineFormatError)));
    }

    #[test]
    fn test_candidate_from_str_invalid_format_missing_typ() {
        let candidate = "candidate:1 1 udp 2122252543 192.168.1.5 54400 host typ";
        let attr = Attribute::from_str(candidate);

        assert!(matches!(attr, Err(Error::InvalidCandidateFormatError)));
    }

    #[test]
    fn test_candidate_from_str_invalid_candidate_type() {
        let candidate = "candidate:1 1 udp 2122252543 192.168.1.5 54400 typ Guest";
        let attr = Attribute::from_str(candidate);

        assert!(matches!(attr, Err(Error::InvalidCandidateTypeError)));
    }

    #[test]
    fn test_candidate_from_str_invalid_priority() {
        let candidate = "candidate:1 1 udp priority 192.168.1.5 54400 typ host";
        let attr = Attribute::from_str(candidate);

        assert!(matches!(attr, Err(Error::InvalidPriorityError)));
    }

    #[test]
    fn test_candidate_from_str_invalid_port() {
        let candidate = "candidate:1 1 udp 2122252543 192.168.1.5 port typ host";
        let attr = Attribute::from_str(candidate);

        assert!(matches!(attr, Err(Error::InvalidPortError)));
    }

    #[test]
    fn test_candidate_from_str_invalid_component_id() {
        let candidate = "candidate:1 id udp 2122252543 192.168.1.5 54400 typ host";
        let attr = Attribute::from_str(candidate);
        assert!(matches!(attr, Err(Error::InvalidComponentIdError)));
    }

    #[test]
    fn test_display_candidate() {
        let candidate = Candidate::new(
            CandidateType::Host,
            2122252543,
            "192.168.1.5".into(),
            54400,
            1,
            "1".into(),
            "udp".into(),
        );

        let attr = Attribute::Candidate(candidate);
        let expected = "a=candidate:1 1 udp 2122252543 192.168.1.5 54400 typ host";
        assert_eq!(attr.to_string(), expected);
    }
}
