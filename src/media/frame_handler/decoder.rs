use super::error::FrameError as Error;
use openh264::decoder::Decoder as H264Decoder;
use openh264::formats::YUVSource;

/// A basic H.264 video decoder using the `OpenH264` library.
///
/// This struct wraps the `OpenH264` decoder and provides a simple way to
/// decode encoded H.264 frame data (`&[u8]`) into a YUV.
///
/// Typically, this is used on the receiving side of a video stream,
/// after collecting and reassembling encoded RTP/UDP packets into a full frame.
pub struct Decoder {
    decoder: H264Decoder,
}

impl Decoder {
    /// Creates a new H.264 decoder instance.
    ///
    /// This initializes an internal `OpenH264` decoder with default settings.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DecoderInitializationError`] if the `OpenH264` library
    /// fails to create the decoder instance.
    pub fn new() -> Result<Self, Error> {
        let decoder = H264Decoder::new().map_err(|_| Error::DecoderInitializationError)?;
        Ok(Self { decoder })
    }

    /// Decodes a single H.264-encoded frame into raw YUV data.
    ///
    /// Takes a slice of encoded H.264 bytes and attempts to decode it
    /// into a `RGB` frame data.
    /// The decoded frame data can then be displayed.
    ///
    /// # Errors
    ///
    /// - [`Error::EmptyFrameError`] — if the provided data does not contain
    ///   a complete frame or yields no output.
    /// - [`Error::DecodingError`] — if decoding fails due to invalid or
    ///   corrupted data.
    pub fn decode_frame(&mut self, encoded_data: &[u8]) -> Result<(Vec<u8>, usize, usize), Error> {
        match self.decoder.decode(encoded_data) {
            Ok(Some(yuv)) => {
                let (width, height) = yuv.dimensions();

                let mut rgb8_data = vec![0u8; width * height * 3];
                yuv.write_rgb8(&mut rgb8_data);

                Ok((rgb8_data, width, height))
            }
            Ok(None) => Err(Error::EmptyFrameError),
            Err(_) => Err(Error::DecodingError),
        }
    }
}
