use super::Attribute;
use super::error::SdpError as Error;
use crate::ice::Candidate;
use std::collections::HashSet;
use std::fmt::Display;
use std::str::FromStr;
/*
#[derive(Debug, PartialEq, Eq)]
pub enum MediaDescriptionError {
    InvalidAttributeFormat,
    InvalidFormat,
}

impl Display for MediaDescriptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAttributeFormat => write!(f, "Invalid attribute format"),
            Self::InvalidFormat => {
                write!(f, "Invalid format for this media description")
            }
        }
    }
}

impl std::error::Error for MediaDescriptionError {}

 */

pub struct MediaDescription {
    pub media_type: String,
    pub port: u16,
    pub protocol: String,
    pub fmts: HashSet<u8>,
    pub attributes: Vec<Attribute>,
}

impl FromStr for MediaDescription {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s_vec: Vec<&str> = s.split_whitespace().collect();
        if s_vec.len() < 4 {
            return Err(Error::InvalidMediaDescriptionFormatError);
        }

        let port = s_vec[1]
            .parse::<u16>()
            .map_err(|_| Error::InvalidPortError)?;

        let mut parsed_fmt = HashSet::new();
        for f_string in &s_vec[3..] {
            let fmt = f_string.parse::<u8>().map_err(|_| Error::InvalidFmtError)?;
            parsed_fmt.insert(fmt);
        }

        Ok(Self::new(
            s_vec[0].into(),
            port,
            s_vec[2].into(),
            parsed_fmt,
        ))
    }
}

impl Display for MediaDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "m={} {} {}", self.media_type, self.port, self.protocol)?;

        for fmt in &self.fmts {
            write!(f, " {fmt}")?;
        }

        for attr in &self.attributes {
            write!(f, "\n {attr}")?;
        }

        Ok(())
    }
}

impl MediaDescription {
    #[must_use]
    pub const fn new(media_type: String, port: u16, protocol: String, fmts: HashSet<u8>) -> Self {
        Self {
            media_type,
            port,
            protocol,
            fmts,
            attributes: Vec::new(),
        }
    }

    pub fn add_attribute(&mut self, attr: Attribute) -> Result<(), Error> {
        if let Attribute::RTPMap(fmt, _, _, _) = &attr
            && !self.fmts.contains(fmt)
        {
            return Err(Error::UnmatchingMediaDescriptionAndAttributeError);
        }

        self.attributes.push(attr);
        Ok(())
    }

    #[must_use]
    pub fn get_candidates(&self) -> Vec<Candidate> {
        let mut candidates: Vec<Candidate> = Vec::new();

        for attr in &self.attributes {
            if let Attribute::Candidate(candidate) = attr {
                candidates.push(candidate.clone());
            }
        }

        candidates
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdp::attribute::Attribute;

    #[test]
    fn test_from_str_valid_media_description() {
        let line = "audio 5004 RTP/AVP 96 97";
        let md = MediaDescription::from_str(line).unwrap();

        assert_eq!(md.media_type, "audio");
        assert_eq!(md.port, 5004);
        assert_eq!(md.protocol, "RTP/AVP");
        assert!(md.fmts.contains(&96));
        assert!(md.fmts.contains(&97));
    }

    #[test]
    fn test_from_str_invalid_format_missing_fields() {
        let line = "audio 5004 RTP/AVP";
        let result = MediaDescription::from_str(line);
        assert!(matches!(result, Err(Error::InvalidMediaDescriptionFormatError)));
    }

    #[test]
    fn test_from_str_invalid_port() {
        let line = "audio notaport RTP/AVP 96";
        let result = MediaDescription::from_str(line);
        assert!(matches!(result, Err(Error::InvalidPortError)));
    }

    #[test]
    fn test_from_str_invalid_fmt() {
        let line = "audio 5004 RTP/AVP x";
        let result = MediaDescription::from_str(line);
        assert!(matches!(result, Err(Error::InvalidFmtError)));
    }

    #[test]
    fn test_add_attribute_valid() {
        let mut fmts = HashSet::new();
        fmts.insert(96);
        let mut md = MediaDescription::new("audio".into(), 5004, "RTP/AVP".into(), fmts);

        let attr = Attribute::RTPMap(96, "opus".into(), 48000, Some("2".into()));
        assert!(md.add_attribute(attr).is_ok());
        assert_eq!(md.attributes.len(), 1);
    }

    #[test]
    fn test_add_attribute_invalid_fmt() {
        let mut fmts = HashSet::new();
        fmts.insert(97);
        let mut md = MediaDescription::new("audio".into(), 5004, "RTP/AVP".into(), fmts);

        let attr = Attribute::RTPMap(96, "opus".into(), 48000, Some("2".into()));
        let result = md.add_attribute(attr);
        assert!(matches!(result, Err(Error::UnmatchingMediaDescriptionAndAttributeError)));
    }
}

