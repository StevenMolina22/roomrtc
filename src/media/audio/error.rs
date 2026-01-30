use std::fmt::Display;

#[derive(Debug)]
pub enum AudioError {
    InputDeviceError,
    MapError(String),
    EncoderInitializationError,
    EncodingError(String),
    DecoderInitializationError,
    DecodingError(String),
}

impl Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            AudioError::InputDeviceError => write!(f, "Error: no input device available"),
            AudioError::MapError(e) => write!(f, "Error: {}", e),
            AudioError::EncoderInitializationError => write!(f, "Error: failed to initialize microphone encoder"),
            AudioError::EncodingError(e) => write!(f, "Error: failed to encode microphone file ({e})"),
            AudioError::DecoderInitializationError => write!(f, "Error: failed to initialize microphone decoder"),
            AudioError::DecodingError(e) => write!(f, "Error: failed to decode microphone file ({e})"),
        }
    }
}