use crate::media::frame_handler::Frame;
use crate::media::frame_handler::FrameError as Error;
use yuv::{
    YuvChromaSubsampling, YuvConversionMode, YuvPlanarImageMut, YuvRange, YuvStandardMatrix,
};
pub fn rgb_to_yuv420<'a>(rgb_frame: &Frame) -> Result<YuvPlanarImageMut<'a, u8>, Error> {
    let mut yuv_img = YuvPlanarImageMut::alloc(
        rgb_frame.width as u32,
        rgb_frame.height as u32,
        YuvChromaSubsampling::Yuv420,
    );

    yuv::rgb_to_yuv420(
        &mut yuv_img,
        &rgb_frame.data,
        (rgb_frame.width as u32) * 3,
        YuvRange::Limited,
        YuvStandardMatrix::Bt709,
        YuvConversionMode::Balanced,
    )
    .map_err(|e| Error::EncodingError(e.to_string()))?;

    Ok(yuv_img)
}
