use super::FrameError as Error;
use opencv::{imgproc, };
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
    /// The implementation uses OpenCV to reinterpret the provided
    /// bytes as a single-channel Mat with height = 3/2 * height
    /// (I420 layout) and then converts the color using
    /// `cv::cvtColor`. On success returns a new `Frame` containing
    /// RGB bytes and the same width/height/id.
    pub fn to_rgb(&self) -> Result<Self, Error> {
        let temp_mat =
            Mat::from_slice(&self.data).map_err(|_| Error::UnableToCreateFrameFromYUVError)?;

        let yuv_mat = temp_mat
            .reshape(1, (self.height * 3 / 2) as i32)
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
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 16 {
            return None;
        }

        let id = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
        let width = usize::from_le_bytes(bytes[8..12].try_into().ok()?);
        let height = usize::from_le_bytes(bytes[12..16].try_into().ok()?);

        let data = bytes[16..].to_vec();

        Some(Self { data, width, height, id })
    }

    /// Serialize the `Frame` into bytes in the same layout consumed by
    /// `from_bytes`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16 + self.data.len());
        buf.extend_from_slice(&self.id.to_le_bytes());
        buf.extend_from_slice(&(self.width as u32).to_le_bytes());
        buf.extend_from_slice(&(self.height as u32).to_le_bytes());
        buf.extend_from_slice(&self.data);
        buf
    }
}
