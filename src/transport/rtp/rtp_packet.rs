use std::default::Default;

/// RTP packet used by the project transport layer.
///
/// This struct models a simplified RTP-like packet used on top of the
/// project's UDP sockets. The packet uses a custom binary layout
/// that packs a small header followed by payload bytes.
///
/// # Custom Header Format
/// The packet serializes to a 33-byte header plus payload:
/// - Byte 0: version (2 bits in upper bits)
/// - Byte 1: marker flag (indicates last chunk of a frame)
/// - Byte 2: total_chunks (number of chunks in complete frame)
/// - Byte 3: is_i_frame flag (1 for I-frame, 0 for P-frame)
/// - Byte 4: payload_type
/// - Bytes 5-12: sequence_number (u64, big-endian)
/// - Bytes 13-28: timestamp (u128, big-endian)
/// - Bytes 29-32: ssrc (u32, big-endian)
/// - Bytes 33+: payload data
#[derive(Debug, Clone, Default)]
pub struct RtpPacket {
    /// Packet format version (typically 2).
    pub version: u8,
    
    /// Marker flag: 1 if this is the last packet of a frame, 0 otherwise.
    pub marker: u8,

    /// Total number of chunks (packets) that compose the complete frame.
    pub total_chunks: u8,

    /// Flag indicating if this frame is an I-frame (intra/keyframe) or P-frame (delta).
    pub is_i_frame: bool,

    /// RTP payload type identifier.
    pub payload_type: u8,

    /// Sequence number for ordering and loss detection.
    pub sequence_number: u64,

    /// RTP timestamp associated with the media sample/frame.
    pub timestamp: u128,

    /// Synchronization source (SSRC) identifier.
    pub ssrc: u32,

    /// Encoded media payload bytes.
    pub payload: Vec<u8>,
}

impl RtpPacket {
    /// Encode the packet into a byte sequence suitable for network transmission.
    ///
    /// Serializes the packet header and payload into a custom binary format
    /// with all multi-byte integers in network byte order (big-endian). The
    /// resulting buffer can be sent over UDP and reconstructed with `from_bytes`.
    ///
    /// # Returns
    /// A byte vector containing the serialized packet (33 bytes + payload length).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(33 + self.payload.len());

        buf.push(self.version << 6);
        buf.push(self.marker);
        buf.extend_from_slice(&self.total_chunks.to_be_bytes());
        buf.push(self.is_i_frame as u8);
        buf.push(self.payload_type);
        buf.extend_from_slice(&self.sequence_number.to_be_bytes());
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.extend_from_slice(&self.ssrc.to_be_bytes());
        buf.extend_from_slice(&self.payload);

        buf
    }

    /// Decode an `RtpPacket` from a byte slice.
    ///
    /// Parses a byte slice previously produced by `to_bytes` and reconstructs
    /// the original packet structure. All multi-byte integers are decoded from
    /// network byte order (big-endian).
    ///
    /// # Parameters
    /// - `data`: byte slice containing the serialized packet.
    ///
    /// # Returns
    /// `Some(RtpPacket)` if the slice is valid and at least 33 bytes long, `None` otherwise.
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 33 {
            return None;
        }

        let version = data[0] >> 6;
        let marker = data[1];
        let total_chunks = data[2];
        let is_i_frame = data[3];
        let payload_type = data[4];
        let sequence_number = u64::from_be_bytes(array_from_slice(&data[5..13]));
        let timestamp = u128::from_be_bytes(array_from_slice(&data[13..29]));
        let ssrc = u32::from_be_bytes(array_from_slice::<4>(&data[29..33]));
        let payload = data[33..].to_vec();

        Some(Self {
            version,
            marker,
            total_chunks,
            is_i_frame: is_i_frame != 0,
            payload_type,
            sequence_number,
            timestamp,
            ssrc,
            payload,
        })
    }
}

/// Helper function to copy `N` bytes from a slice into a fixed-size array.
///
/// # Parameters
/// - `slice`: byte slice to copy from (must have at least N bytes).
///
/// # Returns
/// A fixed-size array containing the first N bytes.
fn array_from_slice<const N: usize>(slice: &[u8]) -> [u8; N] {
    let mut arr = [0u8; N];
    arr.copy_from_slice(&slice[..N]);
    arr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtp_packet_serialization_roundtrip() {
        let original_packet = RtpPacket {
            version: 2,
            marker: 0,
            total_chunks: 15,      
            is_i_frame: true,  
            payload_type: 96,
            sequence_number: 100,
            timestamp: 5000,
            ssrc: 1234,
            payload: vec![0xAA, 0xBB],
        };

        let bytes = original_packet.to_bytes();
        assert_eq!(bytes.len(), 26);
        
        assert_eq!(bytes[2], 1, "El booleano true debería ser 1 en binario");

        let deserialized = RtpPacket::from_bytes(&bytes).unwrap();

        assert_eq!(original_packet.total_chunks, deserialized.total_chunks);
        assert_eq!(original_packet.is_i_frame, deserialized.is_i_frame);
        assert!(deserialized.is_i_frame);
    }

    #[test]
    fn test_bool_false_serialization() {
        let mut packet = RtpPacket::default();
        packet.is_i_frame = false;

        let bytes = packet.to_bytes();
        assert_eq!(bytes[2], 0, "False in binary should be 0");

        let deserialized = RtpPacket::from_bytes(&bytes).unwrap();
        assert!(!deserialized.is_i_frame);
    }
}
