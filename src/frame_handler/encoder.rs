use openh264::encoder::{EncodedBitStream, Encoder as H264Encoder};
use openh264::formats::YUVBuffer;
use super::{frame::Frame, error::FrameError as Error};

/// A basic H.264 video encoder using the OpenH264 library.
///
/// This struct wraps the OpenH264 encoder and provides a simple way to
/// compress raw YUV frames into H.264-encoded byte chunks.
/// These chunks can be sent over the network (e.g. via UDP) and later
/// reassembled and decoded on the receiving side.
pub struct Encoder {
    encoder: H264Encoder,
    max_chunk_size: usize
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
    pub fn new() -> Result<Self, Error> {
        let encoder = H264Encoder::new().map_err(|_| Error::EncoderInitializationError)?;
        Ok(Self { encoder, max_chunk_size: 1200 })
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
    pub fn encode_frame(&mut self, frame: Frame) -> Result<Vec<Vec<u8>>, Error> {
        let yuv = YUVBuffer::from_vec(frame.data, frame.width, frame.height);
        let nalus = self.encoder.encode(&yuv).map_err(|_| Error::EncodingError)?;
        let chunks = generate_chunks_from_nalus(nalus, self.max_chunk_size);

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