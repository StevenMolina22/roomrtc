use openh264::decoder::{DecodedYUV, Decoder as H264Decoder};
use super::error::FrameError as Error;

/// A basic H.264 video decoder using the OpenH264 library.
///
/// This struct wraps the OpenH264 decoder and provides a simple way to
/// decode encoded H.264 frame data (`&[u8]`) into a raw YUV image (`DecodedYUV`).
///
/// Typically, this is used on the receiving side of a video stream,
/// after collecting and reassembling encoded RTP/UDP packets into a full frame.
pub struct Decoder {
    decoder: H264Decoder,
}

impl Decoder {
    /// Creates a new H.264 decoder instance.
    ///
    /// This initializes an internal OpenH264 decoder with default settings.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DecoderInitializationError`] if the OpenH264 library
    /// fails to create the decoder instance.
    pub fn new() -> Result<Self, Error> {
        let decoder = H264Decoder::new().map_err(|_| Error::DecoderInitializationError)?;
        Ok(Self { decoder })
    }

    /// Decodes a single H.264-encoded frame into raw YUV data.
    ///
    /// Takes a slice of encoded H.264 bytes and attempts to decode it
    /// into a [`DecodedYUV`] frame.
    /// The decoded frame can then be converted to RGB/BGR for display
    /// or further processed.
    ///
    /// # Errors
    ///
    /// - [`Error::EmptyFrameError`] — if the provided data does not contain
    ///   a complete frame or yields no output.
    /// - [`Error::DecodingError`] — if decoding fails due to invalid or
    ///   corrupted data.
    pub fn decode_frame(&mut self, encoded_data: &[u8]) -> Result<DecodedYUV<'_>, Error> {
        match self.decoder.decode(encoded_data) {
            Ok(Some(yuv)) => Ok(yuv),
            Ok(None) => Err(Error::EmptyFrameError),
            Err(_) => Err(Error::DecodingError),
        }
    }
}