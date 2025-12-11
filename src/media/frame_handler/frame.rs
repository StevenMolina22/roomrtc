use super::FrameError as Error;
use opencv::{imgproc, core};
use opencv::prelude::*;

/// An in-memory video frame used by the frame handler.
///
/// `Frame` represents raw (decoded) frame data or intermediate
/// conversions (for example, RGB data produced from YUV input). The
/// struct carries the pixel bytes, the frame dimensions and an id.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Raw pixel bytes.
    pub data: Vec<u8>,

    /// Width in pixels of the frame data.
    pub width: usize,

    /// Height in pixels of the frame data.
    pub height: usize,

}

impl Frame {
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
        if bytes.len() < 8 {
            return None;
        }

        let width = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let height = u32::from_le_bytes(bytes[4..8].try_into().ok()?);

        let data = bytes[8..].to_vec();

        Some(Self {
            data,
            width: width as usize,
            height: height as usize,
        })
    }

    /// Serialize the `Frame` into bytes in the same layout consumed by
    /// `from_bytes`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::with_capacity(8 + self.data.len());
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
        };

        let bytes = original_frame
            .to_bytes()
            .expect("Failed to serialize frame");

        assert_eq!(bytes.len(), 8 + 6);

        let deserialized_frame_option = Frame::from_bytes(&bytes);

        assert!(
            deserialized_frame_option.is_some(),
            "Deserialization failed and returned None"
        );

        let deserialized_frame = deserialized_frame_option.unwrap();

        assert_eq!(original_frame, deserialized_frame);
    }

    #[test]
    fn test_frame_with_empty_data() {
        let original_frame = Frame {
            data: vec![],
            width: 640,
            height: 480,
        };

        let bytes = original_frame
            .to_bytes()
            .expect("Failed to serialize frame");

        assert_eq!(bytes.len(), 8);

        let deserialized_frame =
            Frame::from_bytes(&bytes).expect("Deserialization of empty frame failed");

        assert_eq!(original_frame, deserialized_frame);
    }

    #[test]
    fn test_from_bytes_too_short() {
        let short_bytes: &[u8] = &[0; 7];
        let result = Frame::from_bytes(short_bytes);
        assert!(result.is_none());
    }
}