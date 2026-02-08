use super::error::FrameError as Error;
use openh264::decoder::Decoder as H264Decoder;
use openh264::formats::YUVSource;

/// A basic H.264 video decoder using the `OpenH264` library.
///
/// This struct wraps the `OpenH264` decoder and provides a simple way to
/// decode encoded H.264 frame data (`&[u8]`) into RGB bytes.
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

    /// Decodes a single H.264-encoded frame into raw RGB data.
    ///
    /// Takes a slice of encoded H.264 bytes, decodes it to YUV, and converts
    /// the result to RGB bytes along with the frame dimensions.
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
                let (y_stride, u_stride, v_stride) = yuv.strides();

                let yuv_img = yuv::YuvPlanarImage {
                    y_plane: yuv.y(),
                    y_stride: y_stride as u32,
                    u_plane: yuv.u(),
                    u_stride: u_stride as u32,
                    v_plane: yuv.v(),
                    v_stride: v_stride as u32,
                    width: width as u32,
                    height: height as u32,
                };

                let mut rgb = vec![0u8; width * height * 3];

                yuv::yuv420_to_rgb(
                    &yuv_img,
                    &mut rgb,
                    (width * 3) as u32,
                    yuv::YuvRange::Limited,
                    yuv::YuvStandardMatrix::Bt709,
                )
                .map_err(|e| Error::DecodingError(e.to_string()))?;

                Ok((rgb, width, height))
            }
            Ok(None) => Err(Error::EmptyFrameError),
            Err(e) => Err(Error::DecodingError(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Decoder, Error};

    #[test]
    fn test_decoder_new() {
        let d = Decoder::new();
        assert!(d.is_ok(), "Decoder::new() should succeed");
    }

    #[test]
    fn test_decode_invalid_data_returns_error() {
        let mut d = match Decoder::new() {
            Ok(dec) => dec,
            Err(e) => panic!("failed to create decoder: {e:?}"),
        };

        let res = d.decode_frame(&[0u8, 1, 2, 3, 4]);
        assert!(res.is_err(), "decoding invalid data should return an error");

        match res {
            Err(Error::DecodingError(_)) | Err(Error::EmptyFrameError) => {}
            Err(e) => panic!("unexpected error variant: {e:?}"),
            Ok(_) => panic!("expected error decoding invalid data, got Ok"),
        }
    }
}
