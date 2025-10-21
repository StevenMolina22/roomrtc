use std::fmt::Display;

#[derive(Debug)]
pub enum SdpErrors {
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

impl Display for SdpErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SdpErrors::InvalidPriorityError => write!(f, "Error: \"Invalid priority\""),
            SdpErrors::InvalidPortError => write!(f, "Error: \"Invalid port number\""),
            SdpErrors::InvalidComponentIdError => write!(f, "Error: \"Invalid component id\""),
            SdpErrors::InvalidLineFormatError => write!(f, "Error: \"Invalid line format\""),
            SdpErrors::InvalidCandidateFormatError => write!(f, "Error: \"Invalid candidate format\""),
            SdpErrors::InvalidAttributeFormatError => write!(f, "Error: \"Invalid attribute format\""),
            SdpErrors::InvalidCandidateTypeError => write!(f, "Error: \"Invalid candidate type\""),
            SdpErrors::InvalidRtpMapFormatError => write!(f, "Error: \"Invalid RTP map format\""),
            SdpErrors::MissingEncodingNameError => write!(f, "Error: \"Missing encoding name\""),
            SdpErrors::MissingClockRateError => write!(f, "Error: \"Missing clock rate\""),
            SdpErrors::InvalidClockRateParsingError => write!(f, "Error: \"Invalid clock rate parsing error\""),
            SdpErrors::ExtraRtpFieldsError => write!(f, "Error: \"Extra RTP fields\""),
            SdpErrors::InvalidCandidateParsingError => write!(f, "Error: \"Invalid Candidate parsing error\""),
            SdpErrors::MissingMediaDescriptionError => write!(f, "Error: \"Missing media description\""),
            SdpErrors::InvalidMediaDescriptionFormatError => write!(f, "Error: \"Invalid media description\""),
            SdpErrors::InvalidMediaDescriptionAttributeFormat => write!(f, "Error: \"Invalid media description attribute format\""),
            SdpErrors::InvalidFmtError => write!(f, "Error: \"Invalid fmt error\""),

        }
    }
}