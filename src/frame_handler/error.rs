use std::fmt::Display;

/// Represents all possible errors that can occur during
/// the encoding and decoding of video frames.
///
/// This enum is used to describe both encoder and decoder
/// initialization and runtime issues in a simple, unified way.
#[derive(Eq, PartialEq, Debug)]
pub enum FrameError {
    ///Error returned when the encoder failed to initialize.
    EncoderInitializationError,

    ///Error returned when a frame failed to be encoded.
    EncodingError,

    ///Error returned when the decoder failed to initialize.
    DecoderInitializationError,

    ///Error returned when a frame failed to be decoded.
    DecodingError,

    /// Error returned when no frame data is provided to the decoder.
    EmptyFrameError,

    /// Error returned when unable to create a frame from YUV data.
    UnableToCreateFrameFromYUVError,

    /// Error returned when reshaping a frame fails.
    ReshapingFrameError,

    /// Error returned when converting a frame from YUV to RGB fails.
    TypeConversionError,

    /// Error returned when converting a frame to bytes fails.
    BytesConversionError,
}

impl Display for FrameError {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> Result<(), ::std::fmt::Error> {
        match self {
            Self::EncoderInitializationError => {
                write!(fmt, "Error: failed to intialize encoder")
            }
            Self::EncodingError => write!(fmt, "Error: failed to encode frame"),
            Self::DecoderInitializationError => {
                write!(fmt, "Error: failed to initialize decoder")
            }
            Self::DecodingError => write!(fmt, "Error: failed to decode frame"),
            Self::EmptyFrameError => write!(fmt, "Error: no frame was provided"),
            Self::UnableToCreateFrameFromYUVError => {
                write!(fmt, "Error: failed to create a frame from yuv vec.")
            }
            Self::ReshapingFrameError => write!(fmt, "Error: failed to reshape frame"),
            Self::TypeConversionError => {
                write!(fmt, "Error: failed to convert frame from yuv to rgb")
            }
            Self::BytesConversionError => {
                write!(fmt, "Error: failed to convert frame to bytes")
            }
        }
    }
}
