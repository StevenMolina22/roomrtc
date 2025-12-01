mod decoder;
mod encoded_frame;
mod encoder;
mod error;
mod frame;

pub use decoder::Decoder;
pub use encoded_frame::EncodedFrame;
pub use encoder::Encoder;
pub use error::FrameError;
pub use frame::Frame;

#[cfg(test)]
mod tests {
    use super::{Decoder, Encoder, Frame};
    use crate::config::MediaConfig;

    #[test]
    fn test_frame_handlers_integration() {
        // Set up config and a synthetic frame
        let width = 640;
        let height = 480;

        #[allow(clippy::cast_precision_loss)]
        let media_config = MediaConfig {
            camera_index: 0,
            frame_width: width as f64,
            frame_height: height as f64,
            frame_rate: 30,
            h264_idr_interval: 15,
            rtp_max_chunk_size: 1200,
            default_ssrc: 42,
            rtp_payload_type: 111,
            codec_name: "H264".to_string(),
            clock_rate: 48000,
            rtp_version: 2,
            media_type: "video".to_string(),
            media_protocol: "RTP/AVP".to_string(),
        };

        // Generate a synthetic RGB frame
        let frame_data = vec![128u8; width * height * 3];
        let original_frame = Frame {
            data: frame_data,
            width,
            height,
            id: 1,
        };

        let mut encoder = Encoder::new(&media_config).expect("Failed to create encoder");
        let mut decoder = Decoder::new().expect("Failed to create decoder");

        // Encode the frame, re-assemble it, and decode it
        let enc_frame = encoder
            .encode_frame(&original_frame)
            .expect("Encoding failed");

        let reassembled_data = enc_frame.chunks.concat();

        let decoded_result = decoder.decode_frame(&reassembled_data);

        assert!(!enc_frame.chunks.is_empty(), "Encoder produced no chunks");
        assert!(!reassembled_data.is_empty(), "Re-assembled data is empty");

        assert!(
            decoded_result.is_ok(),
            "Decoder returned an error: {:?}",
            decoded_result.err()
        );

        let (decoded_data, decoded_width, decoded_height) = decoded_result.unwrap();

        assert_eq!(
            decoded_width, width,
            "Decoded width does not match original"
        );
        assert_eq!(
            decoded_height, height,
            "Decoded height does not match original"
        );

        // Verify the data buffer is the correct size for the dimensions
        let expected_len = width * height * 3;
        assert_eq!(
            decoded_data.len(),
            expected_len,
            "Decoded data has incorrect length"
        );
    }
}
