use opencv::imgproc;
use opencv::{prelude::*};
use super::FrameError as Error;

#[derive(Clone)]
pub struct Frame {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub id: u64
}

impl Frame {
    pub fn to_rgb(&self) -> Result<Self, Error> {
        let temp_mat = Mat::from_slice(&self.data)
            .map_err(|_| Error::UnableToCreateFrameFromYUVError)?;

        let yuv_mat = temp_mat
            .reshape(1, (self.height * 3 / 2) as i32)
            .map_err(|_| Error::ReshapingFrameError)?;

        let mut rgb_mat = Mat::default();

        imgproc::cvt_color(
            &yuv_mat,
            &mut rgb_mat,
            imgproc::COLOR_YUV2RGB_I420,
            0
        ).map_err(|_| Error::TypeConversionError)?;

        Ok(
            Self {
                data: rgb_mat.data_bytes().map_err(|_| Error::BytesConversionError)?.to_vec(),
                width: self.width,
                height: self.height,
                id: self.id,
            }
        )
    }

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

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16 + self.data.len());
        buf.extend_from_slice(&self.id.to_le_bytes());
        buf.extend_from_slice(&(self.width as u32).to_le_bytes());
        buf.extend_from_slice(&(self.height as u32).to_le_bytes());
        buf.extend_from_slice(&self.data);
        buf
    }
}

