use super::FrameError as Error;
use opencv::imgproc;
use opencv::prelude::*;

/// An in-memory video frame used by the frame handler.
///
/// `Frame` represents raw (decoded) frame data or intermediate
/// conversions (for example, RGB data produced from YUV input). The
/// struct carries the pixel bytes, the frame dimensions and an id.
#[derive(Clone)]
pub struct Frame {
    /// Raw pixel bytes.
    pub data: Vec<u8>,

    /// Width in pixels of the frame data.
    pub width: usize,

    /// Height in pixels of the frame data.
    pub height: usize,

    /// Identifier for the frame.
    pub id: u64,
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

        imgproc::cvt_color(&yuv_mat, &mut rgb_mat, imgproc::COLOR_YUV2RGB_I420, 0)
            .map_err(|_| Error::TypeConversionError)?;

        Ok(Self {
            data: rgb_mat
                .data_bytes()
                .map_err(|_| Error::BytesConversionError)?
                .to_vec(),
            width: self.width,
            height: self.height,
            id: self.id,
        })
    }

    /// Try to create a `Frame` from a byte buffer produced by
    /// `to_bytes`. Returns `None` if the buffer is too small or not
    /// properly formatted.
    ///
    /// Expected layout:
    /// - bytes 0..8: `id`
    /// - bytes 8..12: `width`
    /// - bytes 12..16: `height`
    /// - bytes 16..: raw pixel bytes
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 16 {
            return None;
        }

        let id = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
        let width = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
        let height = u32::from_le_bytes(bytes[12..16].try_into().ok()?);

        let data = bytes[16..].to_vec();

        Some(Self {
            data,
            width: width as usize,
            height: height as usize,
            id,
        })
    }

    /// Serialize the `Frame` into bytes in the same layout consumed by
    /// `from_bytes`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::with_capacity(16 + self.data.len());
        buf.extend_from_slice(&self.id.to_le_bytes());
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
            data: vec![10, 20, 30, 40, 50, 60], // 2 RGB pixels
            width: 2,
            height: 1,
            id: 12345,
        };

        let bytes = original_frame
            .to_bytes()
            .expect("Failed to serialize frame");

        let deserialized_frame_option = Frame::from_bytes(&bytes);

        assert!(
            deserialized_frame_option.is_some(),
            "Deserialization failed and returned None"
        );

        let deserialized_frame = deserialized_frame_option.unwrap();

        assert_eq!(
            original_frame.id, deserialized_frame.id,
            "Frame ID mismatch"
        );
        assert_eq!(
            original_frame.width, deserialized_frame.width,
            "Frame width mismatch"
        );
        assert_eq!(
            original_frame.height, deserialized_frame.height,
            "Frame height mismatch"
        );
        assert_eq!(
            original_frame.data, deserialized_frame.data,
            "Frame data mismatch"
        );
    }

    #[test]
    fn test_frame_with_empty_data() {
        // Test that a frame with no pixel data can also roundtrip
        let original_frame = Frame {
            data: vec![],
            width: 0,
            height: 0,
            id: 777,
        };

        let bytes = original_frame
            .to_bytes()
            .expect("Failed to serialize frame");
        let deserialized_frame =
            Frame::from_bytes(&bytes).expect("Deserialization of empty frame failed");

        assert_eq!(original_frame.id, deserialized_frame.id);
        assert_eq!(original_frame.width, deserialized_frame.width);
        assert_eq!(original_frame.height, deserialized_frame.height);
        assert_eq!(original_frame.data, deserialized_frame.data);
    }

    #[test]
    fn test_from_bytes_too_short() {
        // Test that we get None if the byte slice is too small
        let short_bytes: &[u8] = &[0; 15];

        let result = Frame::from_bytes(short_bytes);

        assert!(
            result.is_none(),
            "Expected None when deserializing from 15 bytes"
        );
    }

    #[test]
    fn test_from_bytes_exactly_header_size() {
        // We get a valid empty frame if the slice is exactly the 16 bytes header size
        // Corresponds to: id=1, width=2, height=3
        let header_only_bytes: &[u8] = &[
            1, 0, 0, 0, 0, 0, 0, 0, //
            2, 0, 0, 0, //
            3, 0, 0, 0, //
        ];

        let result = Frame::from_bytes(header_only_bytes);

        assert!(
            result.is_some(),
            "Expected Some when deserializing exact header size"
        );
        let frame = result.unwrap();

        assert_eq!(frame.id, 1);
        assert_eq!(frame.width, 2);
        assert_eq!(frame.height, 3);
        assert_eq!(frame.data.len(), 0, "Data should be empty");
    }
}
