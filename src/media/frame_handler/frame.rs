use super::FrameError as Error;
use opencv::prelude::*;
use opencv::{core, imgproc};

/// An in-memory video frame used by the frame handler.
///
/// `Frame` represents raw (decoded) frame data or intermediate
/// conversions (for example, RGB data produced from YUV input). The
/// struct carries the pixel bytes, the frame dimensions and the capture
/// timestamp in milliseconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Raw pixel bytes.
    pub data: Vec<u8>,

    /// Width in pixels of the frame data.
    pub width: usize,

    /// Height in pixels of the frame data.
    pub height: usize,

    /// Instant when the frame was captured.
    pub frame_time: u128,
}

impl Frame {
    /// Convert a YUV I420 frame stored in `self.data` to RGB bytes.
    ///
    /// The implementation uses `OpenCV` to reinterpret the provided
    /// bytes as a single-channel Mat with height = 3/2 * height
    /// (I420 layout) and then converts the color using
    /// `cv::cvtColor`. On success returns a new `Frame` containing
    /// RGB bytes and the same width/height/id.
    pub fn to_rgb(&self) -> Result<Self, Error> {
        let temp_mat =
            Mat::from_slice(&self.data).map_err(|_| Error::UnableToCreateFrameFromYUVError)?;

        let yuv_mat = temp_mat
            .reshape(
                1,
                i32::try_from(self.height * 3 / 2).map_err(|_| Error::DimensionConversionError)?,
            )
            .map_err(|_| Error::ReshapingFrameError)?;

        let mut rgb_mat = Mat::default();

        imgproc::cvt_color(
            &yuv_mat,
            &mut rgb_mat,
            imgproc::COLOR_YUV2RGB_I420,
            0,
            core::AlgorithmHint::ALGO_HINT_DEFAULT,
        )
        .map_err(|_| Error::TypeConversionError)?;

        Ok(Self {
            data: rgb_mat
                .data_bytes()
                .map_err(|_| Error::BytesConversionError)?
                .to_vec(),
            width: self.width,
            height: self.height,
            frame_time: self.frame_time,
        })
    }

    /// Try to create a `Frame` from a byte buffer produced by
    /// `to_bytes`. Returns `None` if the buffer is too small or not
    /// properly formatted.
    ///
    /// Expected layout:
    /// - bytes 0..16: `frame_time` as little-endian `u128`
    /// - bytes 16..20: `width` as little-endian `u32`
    /// - bytes 20..24: `height` as little-endian `u32`
    /// - bytes 24..: raw pixel bytes
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 24 {
            return None;
        }

        let frame_time = u128::from_le_bytes(bytes[0..16].try_into().ok()?);
        let width = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
        let height = u32::from_le_bytes(bytes[20..24].try_into().ok()?);

        let data = bytes[24..].to_vec();

        Some(Self {
            data,
            width: width as usize,
            height: height as usize,
            frame_time,
        })
    }

    /// Serialize the `Frame` into bytes in the same layout consumed by
    /// `from_bytes`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DimensionConversionError`] if width or height
    /// cannot be converted to `u32`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::with_capacity(24 + self.data.len());
        buf.extend_from_slice(&self.frame_time.to_le_bytes());
        buf.extend_from_slice(
            &u32::try_from(self.width)
                .map_err(|_| Error::DimensionConversionError)?
                .to_le_bytes(),
        );
        buf.extend_from_slice(
            &u32::try_from(self.height)
                .map_err(|_| Error::DimensionConversionError)?
                .to_le_bytes(),
        );
        buf.extend_from_slice(&self.data);
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_serialization_roundtrip() {
        let original_frame = Frame {
            data: vec![10, 20, 30, 40, 50, 60],
            width: 2,
            height: 1,
            frame_time: 123456789,
        };

        let bytes = original_frame
            .to_bytes()
            .expect("Failed to serialize frame");

        assert_eq!(bytes.len(), 30);

        let deserialized_frame_option = Frame::from_bytes(&bytes);

        assert!(
            deserialized_frame_option.is_some(),
            "Deserialization failed and returned None"
        );

        if let Some(deserialized_frame) = deserialized_frame_option {
            assert_eq!(original_frame, deserialized_frame);
        }
    }

    #[test]
    fn test_frame_with_empty_data() {
        let original_frame = Frame {
            data: vec![],
            width: 640,
            height: 480,
            frame_time: 99999,
        };

        let bytes = original_frame
            .to_bytes()
            .expect("Failed to serialize frame");

        assert_eq!(bytes.len(), 24);

        let deserialized_frame =
            Frame::from_bytes(&bytes).expect("Deserialization of empty frame failed");

        assert_eq!(original_frame, deserialized_frame);
    }

    #[test]
    fn test_from_bytes_too_short() {
        let short_bytes: &[u8] = &[0; 15];
        let result = Frame::from_bytes(short_bytes);
        assert!(result.is_none());
    }

    #[test]
    fn test_from_bytes_exactly_header_size() {
        // frame_time = 1 (u128), width = 2 (u32), height = 3 (u32), no data
        let header_only_bytes: &[u8] = &[
            // frame_time (1 as little-endian u128)
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            // width (2 as little-endian u32)
            2, 0, 0, 0, // height (3 as little-endian u32)
            3, 0, 0, 0,
        ];

        let result = Frame::from_bytes(header_only_bytes);

        assert!(result.is_some());
        if let Some(frame) = result {
            assert_eq!(frame.frame_time, 1);
            assert_eq!(frame.width, 2);
            assert_eq!(frame.height, 3);
            assert_eq!(frame.data.len(), 0);
        }
    }
}
