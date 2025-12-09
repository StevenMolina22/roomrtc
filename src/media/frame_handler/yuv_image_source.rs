use openh264::formats::YUVSource;
use yuv::YuvPlanarImageMut;

pub struct YuvImgSource<'a> {
    pub(crate) img: &'a YuvPlanarImageMut<'a, u8>,
}

impl<'a> YUVSource for YuvImgSource<'a> {
    fn dimensions(&self) -> (usize, usize) {
        (self.img.width as usize, self.img.height as usize)
    }

    fn strides(&self) -> (usize, usize, usize) {
        (
            self.img.y_stride as usize,
            self.img.u_stride as usize,
            self.img.v_stride as usize,
        )
    }

    fn y(&self) -> &[u8] { self.img.y_plane.borrow() }
    fn u(&self) -> &[u8] { self.img.u_plane.borrow() }
    fn v(&self) -> &[u8] { self.img.v_plane.borrow() }
}