use crate::config::MediaConfig;

use super::{error::FrameError as Error, frame::Frame};
use openh264::encoder::{EncodedBitStream, Encoder as H264Encoder};
use openh264::formats::{RgbSliceU8, YUVBuffer};

/// A basic H.264 video encoder using the OpenH264 library.
///
/// This struct wraps the OpenH264 encoder and provides a simple way to
/// compress raw YUV frames into H.264-encoded byte chunks.
/// These chunks can be sent over the network (e.g. via UDP) and later
/// reassembled and decoded on the receiving side.
pub struct Encoder {
    encoder: H264Encoder,
    frame_count: usize,
    idr_interval: usize,
    max_chunk_size: usize,
}

impl Encoder {
    /// Creates a new H.264 encoder instance.
    ///
    /// Initializes the internal OpenH264 encoder with default parameters.
    /// The `max_chunk_size` defines how large each output chunk can be
    /// (useful when sending frames over UDP or other datagram protocols).
    ///
    /// # Errors
    ///
    /// Returns [`Error::EncoderIntializationError`] if the encoder cannot
    /// be created by the OpenH264 library.
    pub fn new(media_config: &MediaConfig) -> Result<Self, Error> {
        let encoder = H264Encoder::new().map_err(|_| Error::EncoderInitializationError)?;

        Ok(Self {
            encoder,
            frame_count: 0,
            idr_interval: media_config.h264_idr_interval,
            max_chunk_size: media_config.rtp_max_chunk_size,
        })
    }

    /// Encodes a raw frame into H.264 byte chunks.
    ///
    /// Takes a [`Frame`] containing raw YUV data, converts it into a
    /// [`YUVBuffer`], and encodes it using OpenH264.
    /// The result is split into smaller chunks, each up to `max_chunk_size`
    /// bytes, ready for transmission.
    ///
    /// # Errors
    ///
    /// - [`Error::EncodingError`] — if encoding fails due to invalid frame data.
    pub fn encode_frame(&mut self, frame: &Frame) -> Result<Vec<Vec<u8>>, Error> {
        let rgb_source = RgbSliceU8::new(&frame.data, (frame.width, frame.height));
        let yuv = YUVBuffer::from_rgb8_source(rgb_source);

        if self.frame_count % self.idr_interval == 0 {
            self.encoder.force_intra_frame();
        }

        let nalus = self
            .encoder
            .encode(&yuv)
            .map_err(|_| Error::EncodingError)?;
        let chunks = generate_chunks_from_nalus(nalus, self.max_chunk_size);

        self.frame_count += 1;

        Ok(chunks)
    }
}

fn generate_chunks_from_nalus(nalus: EncodedBitStream, max_chunk_size: usize) -> Vec<Vec<u8>> {
    let nalu_units = nalus.to_vec();
    let mut chunks = Vec::new();
    for chunk in nalu_units.chunks(max_chunk_size) {
        chunks.push(chunk.to_vec());
    }
    chunks
}
