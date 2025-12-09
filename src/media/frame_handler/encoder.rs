use std::time::Instant;
use crate::config::MediaConfig;
use crate::media::frame_handler::YuvImgSource;

use super::{EncodedFrame, error::FrameError as Error, frame::Frame};
use openh264::encoder::{BitRate, Complexity, EncodedBitStream, Encoder as H264Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, RateControlMode, UsageType};
use openh264::OpenH264API;
use yuv::{YuvChromaSubsampling, YuvConversionMode, YuvPlanarImageMut, YuvRange, YuvStandardMatrix};

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
    pub fn new(media_config: &MediaConfig) -> Result<Self, Error> {
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(2_500_000))
            .max_frame_rate(FrameRate::from_hz(30.0))
            .usage_type(UsageType::CameraVideoRealTime)
            .intra_frame_period(IntraFramePeriod::from_num_frames(30))
            .complexity(Complexity::Medium)
            .rate_control_mode(RateControlMode::Quality)
            .num_threads(4)
            .skip_frames(true);

        let encoder = H264Encoder::with_api_config(OpenH264API::from_source(), config)
            .map_err(|_| Error::EncoderInitializationError)?;

        Ok(Self {
            encoder,
            max_chunk_size: media_config.rtp_max_chunk_size,
        })
    }

    /// Encodes a raw frame into H.264 byte chunks.
    ///
    /// Takes a [`Frame`] containing raw YUV data, converts it into a
    /// [`YUVBuffer`], and encodes it using `OpenH264`.
    /// The result is split into smaller chunks, each up to `max_chunk_size`
    /// bytes, ready for transmission.
    ///
    /// # Errors
    ///
    /// - [`Error::EncodingError`] — if encoding fails due to invalid frame data.
    pub fn encode_frame(&mut self, frame: &Frame) -> Result<EncodedFrame, Error> {
        let mut yuv_img = YuvPlanarImageMut::alloc(
            frame.width as u32,
            frame.height as u32,
            YuvChromaSubsampling::Yuv420
        );

        yuv::rgb_to_yuv420(
            &mut yuv_img,
            &frame.data,
            (frame.width as u32) * 3,
            YuvRange::Limited,
            YuvStandardMatrix::Bt709,
            YuvConversionMode::Balanced,
        ).map_err(|e| Error::EncodingError(e.to_string()))?;

        let yuv_source = YuvImgSource { img: &yuv_img };

        let nalus = self
            .encoder
            .encode(&yuv_source)
            .map_err(|e| Error::EncodingError(e.to_string()))?;

        let chunks = generate_chunks_from_nalus(&nalus, self.max_chunk_size);
        let frame_type = nalus.frame_type();
        let is_i_frame = frame_type == FrameType::I || frame_type == FrameType::IDR;

        Ok(EncodedFrame {
            chunks,
            frame_time: frame.frame_time,
            width: frame.width,
            height: frame.height,
            is_i_frame,
        })
    }
}

/// Splits the encoded NALUs into smaller chunks based on the
/// `max_chunk_size`.
fn generate_chunks_from_nalus(nalus: &EncodedBitStream, max_chunk_size: usize) -> Vec<Vec<u8>> {
    let nalu_units = nalus.to_vec();
    let mut chunks = Vec::new();
    for chunk in nalu_units.chunks(max_chunk_size) {
        chunks.push(chunk.to_vec());
    }
    chunks
}
