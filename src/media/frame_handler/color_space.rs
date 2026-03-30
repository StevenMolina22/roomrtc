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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_to_yuv420_2x2_valid() {
        // 2x2 RGB image: 4 pixels * 3 bytes = 12 bytes
        // pixels: red, green, blue, white
        let frame = Frame {
            width: 2,
            height: 2,
            frame_time: 0,
            data: vec![
                255, 0, 0, // red
                0, 255, 0, // green
                0, 0, 255, // blue
                255, 255, 255, // white
            ],
        };

        let res = rgb_to_yuv420(&frame);
        assert!(res.is_ok(), "Conversion should succeed for 2x2 frame");

        let yuv = res.unwrap();

        assert_eq!(yuv.width as usize, frame.width);
        assert_eq!(yuv.height as usize, frame.height);

        // Y plane length == width * height
        assert_eq!(yuv.y_plane.borrow().len(), frame.width * frame.height);

        // For YUV420, U and V planes are quarter-size (width/2 * height/2)
        assert_eq!(
            yuv.u_plane.borrow().len(),
            (frame.width / 2) * (frame.height / 2)
        );
        assert_eq!(
            yuv.v_plane.borrow().len(),
            (frame.width / 2) * (frame.height / 2)
        );

        // Ensure Y plane contains non-zero values for colored pixels
        let y_sum: usize = yuv.y_plane.borrow().iter().map(|&v| v as usize).sum();
        assert!(y_sum > 0, "Y plane appears empty");
    }

    #[test]
    fn test_rgb_to_yuv420_insufficient_data_errors() {
        // width*height*3 = 12 but provide fewer bytes -> should error
        let frame = Frame {
            width: 2,
            height: 2,
            frame_time: 0,
            data: vec![255, 0, 0, 0, 255, 0, 0, 0, 255], // 9 bytes only
        };

        let res = rgb_to_yuv420(&frame);
        assert!(
            res.is_err(),
            "Conversion should fail with insufficient data"
        );
    }
}
