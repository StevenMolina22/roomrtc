use super::error::SdpError as Error;
use crate::ice::Candidate;
use crate::ice::CandidateType;
use std::fmt;
use std::str::FromStr;

const CANDIDATE_ATTR_KEY: &str = "candidate";
const RTPMAP_ATTR_KEY: &str = "rtpmap";

#[derive(Clone)]
pub enum Attribute {
    Candidate(Candidate),
    RTPMap(u8, String, u32, Option<String>),
}

impl fmt::Display for Attribute {
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

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once(':') {
            Some((CANDIDATE_ATTR_KEY, value)) => {
                parse_candidate_attr_values(value).map_err(|_| Error::InvalidCandidateParsingError)
            }

            Some((RTPMAP_ATTR_KEY, value)) => parse_rptmap_attr_values(value),
            _ => Err(Error::InvalidRtpMapFormatError),
        }
    }
}

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
