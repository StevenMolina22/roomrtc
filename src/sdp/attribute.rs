use super::error::SdpError as Error;
use crate::ice::Candidate;
use crate::ice::CandidateType;
use std::fmt;
use std::str::FromStr;

/// SDP attribute keys used by the parser.
const CANDIDATE_ATTR_KEY: &str = "candidate";
const RTPMAP_ATTR_KEY: &str = "rtpmap";
const FINGERPRINT_ATTR_KEY: &str = "fingerprint";
const SETUP_ATTR_KEY: &str = "setup";

/// DTLS setup role advertised via the SDP `a=setup` attribute.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DtlsSetupRole {
    ActPass,
    Active,
    Passive,
    HoldConn,
}

impl fmt::Display for DtlsSetupRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::ActPass => "actpass",
            Self::Active => "active",
            Self::Passive => "passive",
            Self::HoldConn => "holdconn",
        };
        write!(f, "{value}")
    }
}

impl FromStr for DtlsSetupRole {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "actpass" => Ok(Self::ActPass),
            "active" => Ok(Self::Active),
            "passive" => Ok(Self::Passive),
            "holdconn" => Ok(Self::HoldConn),
            _ => Err(Error::InvalidSetupRoleError),
        }
    }
}

/// Parsed representation of an SDP `a=fingerprint` attribute.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fingerprint {
    algorithm: String,
    bytes: Vec<u8>,
}

impl Fingerprint {
    /// Create a fingerprint from the provided raw bytes.
    #[must_use]
    pub fn from_bytes(algorithm: impl Into<String>, bytes: &[u8]) -> Self {
        Self {
            algorithm: algorithm.into(),
            bytes: bytes.to_vec(),
        }
    }

    /// Parse a colon-separated fingerprint string.
    pub fn from_hash_string(algorithm: impl Into<String>, hash: &str) -> Result<Self, Error> {
        let trimmed_hash = hash.trim();
        if trimmed_hash.is_empty() {
            return Err(Error::MissingFingerprintValueError);
        }

        let mut parsed = Vec::new();
        for part in trimmed_hash.split(':') {
            let cleaned = part.trim();
            if cleaned.is_empty() {
                return Err(Error::InvalidFingerprintFormatError);
            }

            let byte = u8::from_str_radix(cleaned, 16)
                .map_err(|_| Error::InvalidFingerprintFormatError)?;
            parsed.push(byte);
        }

        if parsed.is_empty() {
            return Err(Error::MissingFingerprintValueError);
        }

        Ok(Self {
            algorithm: algorithm.into(),
            bytes: parsed,
        })
    }

    /// Return the algorithm string (e.g. `sha-256`).
    #[must_use]
    pub fn algorithm(&self) -> &str {
        &self.algorithm
    }

    /// Return the raw fingerprint bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Render the fingerprint as uppercase colon-delimited hex.
    #[must_use]
    pub fn hex_value(&self) -> String {
        self.bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<String>>()
            .join(":")
    }
}

/// SDP attribute representation used by this module.
#[derive(Clone)]
pub enum Attribute {
    /// An ICE candidate attribute containing a full `Candidate`.
    Candidate(Candidate),
    // (rtp_payload_type, rtp_codec_name, rtp_clock_rate, rtp_encoding_params)
    RTPMap(u8, String, u32, Option<String>),
    /// DTLS fingerprint attribute.
    Fingerprint(Fingerprint),
    /// DTLS setup attribute.
    Setup(DtlsSetupRole),
}

impl fmt::Display for Attribute {
    /// Render the attribute back to SDP attribute line form.
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
            Self::Fingerprint(fingerprint) => {
                write!(
                    f,
                    "a=fingerprint:{} {}",
                    fingerprint.algorithm(),
                    fingerprint.hex_value()
                )
            }
            Self::Setup(role) => write!(f, "a=setup:{role}"),
        }
    }
}

impl FromStr for Attribute {
    type Err = Error;

    /// Parse an SDP attribute line (without the leading `a=`) into an `Attribute`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once(':') {
            Some((key, value)) if key == CANDIDATE_ATTR_KEY => parse_candidate_attr_values(value),
            Some((key, value)) if key == RTPMAP_ATTR_KEY => parse_rptmap_attr_values(value),
            Some((key, value)) if key == FINGERPRINT_ATTR_KEY => {
                parse_fingerprint_attr_values(value)
            }
            Some((key, value)) if key == SETUP_ATTR_KEY => parse_setup_attr_values(value),
            _ => Err(Error::InvalidAttributeFormatError),
        }
    }
}

/// Parse an `rtpmap` attribute value part into `Attribute::RTPMap`.
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

/// Parse a fingerprint attribute.
fn parse_fingerprint_attr_values(values: &str) -> Result<Attribute, Error> {
    let mut parts = values.split_whitespace();
    let algorithm = parts
        .next()
        .ok_or(Error::InvalidAttributeFormatError)?
        .trim();
    let hash = parts
        .next()
        .ok_or(Error::MissingFingerprintValueError)?
        .trim();

    let fingerprint = Fingerprint::from_hash_string(algorithm, hash)?;
    Ok(Attribute::Fingerprint(fingerprint))
}

/// Parse a setup attribute.
fn parse_setup_attr_values(value: &str) -> Result<Attribute, Error> {
    let role = DtlsSetupRole::from_str(value.trim())?;
    Ok(Attribute::Setup(role))
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
        let line = "rtpmap:96 opus/48000/2/extra";
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
                assert_eq!(cand.priority, 2_122_252_543);
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
            2_122_252_543,
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

    #[test]
    fn test_fingerprint_attribute_roundtrip() -> Result<(), Error> {
        let line = "fingerprint:sha-256 AA:BB:CC:DD";
        let attr = Attribute::from_str(line)?;

        match attr {
            Attribute::Fingerprint(fp) => {
                assert_eq!(fp.algorithm(), "sha-256");
                assert_eq!(fp.hex_value(), "AA:BB:CC:DD");
            }
            _ => panic!("Expected fingerprint attribute"),
        }

        let fp = Fingerprint::from_hash_string("sha-1", "01:02")?;
        assert_eq!(
            Attribute::Fingerprint(fp).to_string(),
            "a=fingerprint:sha-1 01:02"
        );
        Ok(())
    }

    #[test]
    fn test_setup_attribute_roundtrip() -> Result<(), Error> {
        let line = "setup:active";
        let attr = Attribute::from_str(line)?;
        assert!(matches!(attr, Attribute::Setup(DtlsSetupRole::Active)));
        assert_eq!(
            Attribute::Setup(DtlsSetupRole::Passive).to_string(),
            "a=setup:passive"
        );
        Ok(())
    }

    #[test]
    fn test_invalid_fingerprint_format() {
        let line = "fingerprint:sha-256 invalid";
        let attr = Attribute::from_str(line);
        assert!(matches!(attr, Err(Error::InvalidFingerprintFormatError)));
    }
}
