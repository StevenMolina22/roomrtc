use std::fmt::Display;

#[derive(Eq, PartialEq, Debug)]
pub enum SdpError {
    InvalidPriorityError,
    InvalidPortError,
    InvalidComponentIdError,
    InvalidLineFormatError,
    InvalidCandidateFormatError,
    InvalidAttributeFormatError,
    InvalidCandidateTypeError,
    InvalidRtpMapFormatError,
    MissingEncodingNameError,
    MissingClockRateError,
    InvalidClockRateParsingError,
    ExtraRtpFieldsError,
    InvalidCandidateParsingError,
    MissingMediaDescriptionError,
    InvalidMediaDescriptionFormatError,
    InvalidMediaDescriptionAttributeFormat,
    InvalidFmtError,
}

impl Display for SdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SdpError::InvalidPriorityError => write!(f, "Error: \"Invalid priority\""),
            SdpError::InvalidPortError => write!(f, "Error: \"Invalid port number\""),
            SdpError::InvalidComponentIdError => write!(f, "Error: \"Invalid component id\""),
            SdpError::InvalidLineFormatError => write!(f, "Error: \"Invalid line format\""),
            SdpError::InvalidCandidateFormatError => write!(f, "Error: \"Invalid candidate format\""),
            SdpError::InvalidAttributeFormatError => write!(f, "Error: \"Invalid attribute format\""),
            SdpError::InvalidCandidateTypeError => write!(f, "Error: \"Invalid candidate type\""),
            SdpError::InvalidRtpMapFormatError => write!(f, "Error: \"Invalid RTP map format\""),
            SdpError::MissingEncodingNameError => write!(f, "Error: \"Missing encoding name\""),
            SdpError::MissingClockRateError => write!(f, "Error: \"Missing clock rate\""),
            SdpError::InvalidClockRateParsingError => write!(f, "Error: \"Invalid clock rate parsing error\""),
            SdpError::ExtraRtpFieldsError => write!(f, "Error: \"Extra RTP fields\""),
            SdpError::InvalidCandidateParsingError => write!(f, "Error: \"Invalid Candidate parsing error\""),
            SdpError::MissingMediaDescriptionError => write!(f, "Error: \"Missing media description\""),
            SdpError::InvalidMediaDescriptionFormatError => write!(f, "Error: \"Invalid media description\""),
            SdpError::InvalidMediaDescriptionAttributeFormat => write!(f, "Error: \"Invalid media description attribute format\""),
            SdpError::InvalidFmtError => write!(f, "Error: \"Invalid fmt error\""),

        }
    }
}