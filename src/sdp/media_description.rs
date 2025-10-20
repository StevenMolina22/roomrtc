use crate::sdp::Attribute;
use crate::ice::Candidate;
use std::collections::HashSet;
use std::fmt::Display;
use std::str::FromStr;

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

pub struct MediaDescription {
    pub media_type: String,
    pub port: u16,
    pub protocol: String,
    pub fmts: HashSet<u8>,
    pub attributes: Vec<Attribute>,
}

impl FromStr for MediaDescription {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s_vec: Vec<&str> = s.split_whitespace().collect();
        if s_vec.len() < 4 {
            return Err(());
        }

        let port = s_vec[1].parse::<u16>().map_err(|_| ())?;

        let mut parsed_fmt = HashSet::new();
        for f_string in &s_vec[3..] {
            let fmt = f_string.parse::<u8>().map_err(|_| ())?;
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

    pub fn add_attribute(&mut self, attr: Attribute) -> Result<(), MediaDescriptionError> {
        if let Attribute::RTPMap(fmt, _, _, _) = &attr
            && !self.fmts.contains(fmt)
        {
            return Err(MediaDescriptionError::InvalidFormat);
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
