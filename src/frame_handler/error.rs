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
    UnableToCreateFrameFromYUVError,
    ReshapingFrameError,
    TypeConversionError,
    BytesConversionError,
}

impl Display for FrameError {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> Result<(), ::std::fmt::Error> {
        match self {
            FrameError::EncoderInitializationError => write!(fmt, "Error: failed to intialize encoder"),
            FrameError::EncodingError => write!(fmt, "Error: failed to encode frame"),
            FrameError::DecoderInitializationError => write!(fmt, "Error: failed to initialize decoder"),
            FrameError::DecodingError => write!(fmt, "Error: failed to decode frame"),
            FrameError::EmptyFrameError => write!(fmt, "Error: no frame was provided"),
            FrameError::UnableToCreateFrameFromYUVError => write!(fmt, "Error: failed to create a frame from yuv vec."),
            FrameError::ReshapingFrameError => write!(fmt, "Error: failed to reshape frame"),
            FrameError::TypeConversionError => write!(fmt, "Error: failed to convert frame from yuv to rgb"),
            FrameError::BytesConversionError => write!(fmt, "Error: failed to convert frame to bytes"),
        }
    }
}