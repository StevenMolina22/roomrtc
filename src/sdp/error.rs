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

    /// Fingerprint attribute missing required value.
    MissingFingerprintValueError,

    /// Fingerprint attribute has invalid hexadecimal format.
    InvalidFingerprintFormatError,

    /// DTLS setup attribute advertised an unknown role.
    InvalidSetupRoleError,

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
            Self::InvalidPriorityError => write!(f, "Error: \"Invalid priority\""),
            Self::InvalidPortError => write!(f, "Error: \"Invalid port number\""),
            Self::InvalidComponentIdError => write!(f, "Error: \"Invalid component id\""),
            Self::InvalidLineFormatError => write!(f, "Error: \"Invalid line format\""),
            Self::InvalidCandidateFormatError => {
                write!(f, "Error: \"Invalid candidate format\"")
            }
            Self::InvalidAttributeFormatError => {
                write!(f, "Error: \"Invalid attribute format\"")
            }
            Self::InvalidCandidateTypeError => write!(f, "Error: \"Invalid candidate type\""),
            Self::InvalidRtpMapFormatError => write!(f, "Error: \"Invalid RTP map format\""),
            Self::MissingEncodingNameError => write!(f, "Error: \"Missing encoding name\""),
            Self::MissingClockRateError => write!(f, "Error: \"Missing clock rate\""),
            Self::InvalidClockRateParsingError => {
                write!(f, "Error: \"Invalid clock rate parsing error\"")
            }
            Self::ExtraRtpFieldsError => write!(f, "Error: \"Extra RTP fields\""),
            Self::InvalidCandidateParsingError => {
                write!(f, "Error: \"Invalid Candidate parsing error\"")
            }
            Self::MissingFingerprintValueError => {
                write!(f, "Error: \"Missing fingerprint value\"")
            }
            Self::InvalidFingerprintFormatError => {
                write!(f, "Error: \"Invalid fingerprint format\"")
            }
            Self::InvalidSetupRoleError => write!(f, "Error: \"Invalid setup role\""),
            Self::MissingMediaDescriptionError => {
                write!(f, "Error: \"Missing media description\"")
            }
            Self::InvalidMediaDescriptionFormatError => {
                write!(f, "Error: \"Invalid media description\"")
            }
            Self::InvalidMediaDescriptionAttributeFormat => {
                write!(f, "Error: \"Invalid media description attribute format\"")
            }
            Self::InvalidFmtError => write!(f, "Error: \"Invalid fmt error\""),
            Self::UnmatchingMediaDescriptionAndAttributeError => {
                write!(f, "Error: \"Unmatching media description and attribute\"")
            }
        }
    }
}
