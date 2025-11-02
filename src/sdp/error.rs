use std::fmt::Display;

/// Errors returned by the SDP parsing utilities.
///
/// This enum lists the different parsing and validation errors that the
/// SDP module may produce while parsing attributes and media
/// descriptions.
#[derive(Eq, PartialEq, Debug)]
pub enum SdpError {
    /// Priority value could not be parsed or was invalid.
    InvalidPriorityError,

    /// Port value could not be parsed or is invalid.
    InvalidPortError,

    /// Component id could not be parsed or is invalid.
    InvalidComponentIdError,

    /// A general line format is not as expected.
    InvalidLineFormatError,

    /// The `candidate` line does not follow the expected token layout.
    InvalidCandidateFormatError,

    /// Generic attribute formatting error.
    InvalidAttributeFormatError,

    /// The candidate type token is unknown or cannot be parsed.
    InvalidCandidateTypeError,

    /// The `rtpmap` attribute has an invalid format.
    InvalidRtpMapFormatError,

    /// The `rtpmap` attribute is missing the encoding name.
    MissingEncodingNameError,

    /// The `rtpmap` attribute is missing the clock rate.
    MissingClockRateError,

    /// Failed to parse the clock rate value.
    InvalidClockRateParsingError,

    /// Too many fields in the `rtpmap` attribute (more than 3 parts).
    ExtraRtpFieldsError,

    /// Generic candidate parsing failure.
    InvalidCandidateParsingError,

    /// Media description was expected but not found.
    MissingMediaDescriptionError,

    /// Media description line has an invalid format.
    InvalidMediaDescriptionFormatError,

    /// Media description contains attributes with invalid format.
    InvalidMediaDescriptionAttributeFormat,

    /// Payload fmt token is invalid.
    InvalidFmtError,

    /// The attribute format does not match the media description's format.
    UnmatchingMediaDescriptionAndAttributeError,
}

impl Display for SdpError {
    /// Format the error as a short human-readable message.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SdpError::InvalidPriorityError => write!(f, "Error: \"Invalid priority\""),
            SdpError::InvalidPortError => write!(f, "Error: \"Invalid port number\""),
            SdpError::InvalidComponentIdError => write!(f, "Error: \"Invalid component id\""),
            SdpError::InvalidLineFormatError => write!(f, "Error: \"Invalid line format\""),
            SdpError::InvalidCandidateFormatError => {
                write!(f, "Error: \"Invalid candidate format\"")
            }
            SdpError::InvalidAttributeFormatError => {
                write!(f, "Error: \"Invalid attribute format\"")
            }
            SdpError::InvalidCandidateTypeError => write!(f, "Error: \"Invalid candidate type\""),
            SdpError::InvalidRtpMapFormatError => write!(f, "Error: \"Invalid RTP map format\""),
            SdpError::MissingEncodingNameError => write!(f, "Error: \"Missing encoding name\""),
            SdpError::MissingClockRateError => write!(f, "Error: \"Missing clock rate\""),
            SdpError::InvalidClockRateParsingError => {
                write!(f, "Error: \"Invalid clock rate parsing error\"")
            }
            SdpError::ExtraRtpFieldsError => write!(f, "Error: \"Extra RTP fields\""),
            SdpError::InvalidCandidateParsingError => {
                write!(f, "Error: \"Invalid Candidate parsing error\"")
            }
            SdpError::MissingMediaDescriptionError => {
                write!(f, "Error: \"Missing media description\"")
            }
            SdpError::InvalidMediaDescriptionFormatError => {
                write!(f, "Error: \"Invalid media description\"")
            }
            SdpError::InvalidMediaDescriptionAttributeFormat => {
                write!(f, "Error: \"Invalid media description attribute format\"")
            }
            SdpError::InvalidFmtError => write!(f, "Error: \"Invalid fmt error\""),
            SdpError::UnmatchingMediaDescriptionAndAttributeError => {
                write!(f, "Error: \"Unmatching media description and attribute\"")
            }
        }
    }
}
