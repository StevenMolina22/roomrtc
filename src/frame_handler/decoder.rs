use openh264::decoder::{Decoder as H264Decoder};
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
    /// into a [`RGB`] frame data.
    /// The decoded frame data can then be displayed.
    ///
    /// # Errors
    ///
    /// - [`Error::EmptyFrameError`] — if the provided data does not contain
    ///   a complete frame or yields no output.
    /// - [`Error::DecodingError`] — if decoding fails due to invalid or
    ///   corrupted data.
    pub fn decode_frame(&mut self, encoded_data: &[u8]) -> Result<Vec<u8>, Error> {
        match self.decoder.decode(encoded_data) {
            Ok(Some(yuv)) => {
                let (width, height) = yuv.dimensions_uv();
                let full_width = width * 2;
                let full_height = height * 2;
                let mut buffer = vec![0u8; full_width * full_height * 3];

                yuv.write_rgb8(&mut buffer);
                Ok(buffer)
            }
            Ok(None) => Err(Error::EmptyFrameError),
            Err(_) => Err(Error::DecodingError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openh264::encoder::{Encoder as H264Encoder};
    use openh264::formats::YUVBuffer;
    use opencv::{prelude::*, videoio, imgproc, core};

    #[test]
    fn test_decoded_frame_has_the_expected_size() -> Result<(), Error> {
        let mut cam = videoio::VideoCapture::new(0, videoio::CAP_ANY).map_err(|_| Error::DecoderInitializationError)?;
        assert!(videoio::VideoCapture::is_opened(&cam).map_err(|_| Error::DecoderInitializationError)?);

        let mut mat = Mat::default();
        cam.read(&mut mat).map_err(|_| Error::DecodingError)?;
        assert!(!mat.empty());

        let width = mat.cols();
        let height = mat.rows();

        let mut yuv = Mat::default();
        //imgproc::cvt_color(&mat, &mut yuv, imgproc::COLOR_BGR2YUV_I420, 0, core::AlgorithmHint::ALGO_HINT_DEFAULT);
        let yuv_bytes = yuv.data_bytes().map_err(|_| Error::DecodingError)?.to_vec();
        let yuv_buff = YUVBuffer::from_vec(yuv_bytes, width as usize, height as usize);

        let mut encoder = H264Encoder::new().map_err(|_| Error::EncoderInitializationError)?;
        let encoded = encoder.encode(&yuv_buff).map_err(|_| Error::EncodingError)?.to_vec();

        let mut decoder = Decoder::new()?;
        let decoded_rgb = decoder.decode_frame(&encoded)?;

        let expected_len = (width * height * 3) as usize;
        assert_eq!(decoded_rgb.len(), expected_len);
        Ok(())
    }
}
