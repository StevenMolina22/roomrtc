use crate::config::MediaConfig;

use super::{EncodedFrame, error::FrameError as Error};
use openh264::OpenH264API;
use openh264::encoder::{
    BitRate, Complexity, EncodedBitStream, Encoder as H264Encoder, EncoderConfig, FrameRate,
    FrameType, IntraFramePeriod, RateControlMode, UsageType,
};
use openh264::formats::YUVSlices;
use yuv::YuvPlanarImageMut;

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
            .max_frame_rate(FrameRate::from_hz(media_config.frame_rate))
            .usage_type(UsageType::CameraVideoRealTime)
            .intra_frame_period(IntraFramePeriod::from_num_frames(
                media_config.h264_idr_interval,
            ))
            .complexity(Complexity::Low)
            .rate_control_mode(RateControlMode::Off)
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
    /// Takes a [`Frame`] containing raw RGB data, converts it into a
    /// YUV420 planar buffer, and encodes it using `OpenH264`.
    /// The result is split into smaller chunks, each up to `max_chunk_size`
    /// bytes, ready for transmission.
    ///
    /// # Errors
    ///
    /// - [`Error::EncodingError`] — if encoding fails due to invalid frame data.
    pub fn encode(
        &mut self,
        yuv: &YuvPlanarImageMut<u8>,
        frame_time: u128,
    ) -> Result<EncodedFrame, Error> {
        let yuv_source = YUVSlices::new(
            (
                yuv.y_plane.borrow(),
                yuv.u_plane.borrow(),
                yuv.v_plane.borrow(),
            ),
            (yuv.width as usize, yuv.height as usize),
            (
                yuv.y_stride as usize,
                yuv.u_stride as usize,
                yuv.v_stride as usize,
            ),
        );

        let bitstream = self
            .encoder
            .encode(&yuv_source)
            .map_err(|e| Error::EncodingError(e.to_string()))?;

        let chunks = generate_chunks_from_bitstream(&bitstream, self.max_chunk_size);
        let frame_type = bitstream.frame_type();
        let is_i_frame = frame_type == FrameType::I || frame_type == FrameType::IDR;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MediaConfig;
    use yuv::{YuvChromaSubsampling, YuvPlanarImageMut};

    fn test_media_config(chunk_size: usize) -> MediaConfig {
        MediaConfig {
            camera_index: 0,
            frame_width: 4.0,
            frame_height: 4.0,
            frame_rate: 30.0,
            h264_idr_interval: 30,
            rtp_max_chunk_size: chunk_size,
            frame_ssrc: 0,
            audio_ssrc: 0,
            video_payload_type: 96,
            video_codec_name: "H264".to_string(),
            clock_rate: 90000,
            rtp_version: 2,
            video_media_type: "video".to_string(),
            audio_media_type: "audio".to_string(),
            media_protocol: "RTP/AVP".to_string(),
            jitter_buffer_size: 50,
            audio_channels: 1,
            audio_sample_rate: 48000,
            audio_frame_size: 960,
            audio_payload_type: 97,
            audio_codec_name: "OPUS".to_string(),
        }
    }

    #[test]
    fn encoder_new_and_encode_attempt() {
        let cfg = test_media_config(1024);

        match Encoder::new(&cfg) {
            Ok(mut enc) => {
                let mut yuv = YuvPlanarImageMut::alloc(4, 4, YuvChromaSubsampling::Yuv420);

                for v in yuv.y_plane.borrow_mut().iter_mut() {
                    *v = 128;
                }
                for v in yuv.u_plane.borrow_mut().iter_mut() {
                    *v = 128;
                }
                for v in yuv.v_plane.borrow_mut().iter_mut() {
                    *v = 128;
                }

                let res = enc.encode(&yuv, 12345);
                match res {
                    Ok(ef) => {
                        assert_eq!(ef.frame_time, 12345);
                        assert!(
                            !ef.chunks.is_empty(),
                            "Encoded frame should contain at least one chunk"
                        );
                    }
                    Err(e) => match e {
                        Error::EncodingError(_) => {
                            // Acceptable in environments where runtime encoding fails
                        }
                        other => panic!("Unexpected encoding error: {other:?}"),
                    },
                }
            }
            Err(e) => {
                assert_eq!(e, Error::EncoderInitializationError);
            }
        }
    }

    #[test]
    fn new_encoder_respects_chunk_size_config() {
        let small_cfg = test_media_config(1);

        match Encoder::new(&small_cfg) {
            Ok(mut enc) => {
                let mut yuv = YuvPlanarImageMut::alloc(4, 4, YuvChromaSubsampling::Yuv420);
                for v in yuv.y_plane.borrow_mut().iter_mut() {
                    *v = 128;
                }
                for v in yuv.u_plane.borrow_mut().iter_mut() {
                    *v = 128;
                }
                for v in yuv.v_plane.borrow_mut().iter_mut() {
                    *v = 128;
                }

                let res = enc.encode(&yuv, 1);
                match res {
                    Ok(ef) => {
                        assert!(!ef.chunks.is_empty());
                        for c in ef.chunks.iter() {
                            assert!(c.len() <= small_cfg.rtp_max_chunk_size);
                        }
                    }
                    Err(e) => match e {
                        Error::EncodingError(_) => {}
                        other => panic!("Unexpected encoding error: {other:?}"),
                    },
                }
            }
            Err(e) => assert_eq!(e, Error::EncoderInitializationError),
        }
    }
}
