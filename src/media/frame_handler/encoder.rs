use std::time::Instant;
use crate::config::MediaConfig;
use crate::logger::Logger;

use super::{EncodedFrame, error::FrameError as Error};
use openh264::encoder::{BitRate, Complexity, EncodedBitStream, Encoder as H264Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, RateControlMode, UsageType};
use openh264::formats::YUVSlices;
use openh264::OpenH264API;
use yuv::{YuvPlanarImageMut};

/// A basic H.264 video encoder using the `OpenH264` library.
///
/// This struct wraps the `OpenH264` encoder and provides a simple way to
/// compress raw YUV frames into H.264-encoded byte chunks.
/// These chunks can be sent over the network (e.g. via UDP) and later
/// reassembled and decoded on the receiving side.
#[allow(clippy::struct_field_names)]
pub struct Encoder {
    encoder: H264Encoder,
    max_chunk_size: usize,
    logger: Logger,
}

impl Encoder {
    /// Creates a new H.264 encoder instance.
    ///
    /// Initializes the internal `OpenH264` encoder with default parameters.
    /// The `max_chunk_size` defines how large each output chunk can be
    /// (useful when sending frames over UDP or other datagram protocols).
    ///
    /// # Errors
    ///
    /// Returns [`Error::EncoderInitializationError`] if the encoder cannot
    /// be created by the `OpenH264` library.
    pub fn new(media_config: &MediaConfig, logger: Logger) -> Result<Self, Error> {
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(2_500_000))
            .max_frame_rate(FrameRate::from_hz(media_config.frame_rate))
            .usage_type(UsageType::CameraVideoRealTime)
            .intra_frame_period(IntraFramePeriod::from_num_frames(media_config.h264_idr_interval))
            .complexity(Complexity::Low)
            .rate_control_mode(RateControlMode::Off)
            .skip_frames(true);

        let encoder = H264Encoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|_| Error::EncoderInitializationError)?;

        Ok(Self {
            encoder,
            max_chunk_size: media_config.rtp_max_chunk_size,
            logger,
        })
    }

    /// Encodes a raw frame into H.264 byte chunks.
    ///
    /// Takes a [`Frame`] containing raw RGB data, converts it into a
    /// YUV420 planar buffer, and encodes it using `OpenH264`.
    /// The result is split into smaller chunks, each up to `max_chunk_size`
    /// bytes, ready for transmission.
    ///
    /// # Errors
    ///
    /// - [`Error::EncodingError`] — if encoding fails due to invalid frame data.
    pub fn encode(&mut self, yuv: &YuvPlanarImageMut<u8>, frame_time: u128) -> Result<EncodedFrame, Error> {
        let c = Instant::now();

        let yuv_source = YUVSlices::new(
            (
                yuv.y_plane.borrow(),
                yuv.u_plane.borrow(),
                yuv.v_plane.borrow(),
            ),
            (
                yuv.width as usize,
                yuv.height as usize,
            ),
            (
                yuv.y_stride as usize,
                yuv.u_stride as usize,
                yuv.v_stride as usize,
            )
        );

        let bitstream = self
            .encoder
            .encode(&yuv_source)
            .map_err(|e| Error::EncodingError(e.to_string()))?;

        let chunks = generate_chunks_from_bitstream(&bitstream, self.max_chunk_size);
        let frame_type = bitstream.frame_type();
        let is_i_frame = frame_type == FrameType::I || frame_type == FrameType::IDR;

        self.logger.debug(&format!("ENCODER: {}", c.elapsed().as_millis()));
        Ok(EncodedFrame {
            chunks,
            frame_time,
            is_i_frame,
        })
    }
}

/// Splits the encoded NALUs into smaller chunks based on the
/// `max_chunk_size`.
///
/// This helper ensures each chunk is at most `max_chunk_size` bytes,
/// which is useful for transport over datagram protocols like UDP.
fn generate_chunks_from_bitstream(nalus: &EncodedBitStream, max_chunk_size: usize) -> Vec<Vec<u8>> {
    let nalu_units = nalus.to_vec();
    let mut chunks = Vec::new();
    for chunk in nalu_units.chunks(max_chunk_size) {
        chunks.push(chunk.to_vec());
    }
    chunks
}