use std::default::Default;

/// RTP packet used by the project transport layer.
///
/// This struct models a simplified RTP-like packet used on top of the
/// project's UDP sockets. The packet uses a custom binary layout
/// that packs a small header followed by payload
/// bytes.
#[derive(Debug, Clone, Default)]
pub struct RtpPacket {
    /// Packet format version.
    version: u8,

    /// Marker/total-chunks field used by the application.
    pub marker: u16,

    /// Payload type.
    pub(crate) payload_type: u8,

    /// Logical frame identifier for the packet's media frame.
    pub frame_id: u64,

    /// Chunk identifier within the frame.
    pub chunk_id: u64,

    /// Timestamp associated with the sample/frame.
    pub timestamp: u32,

    /// SSRC (synchronization source) identifier.
    pub(crate) ssrc: u32,

    /// Payload bytes of the packet.
    pub payload: Vec<u8>,
}

impl RtpPacket {
    /// Create a new `RtpPacket` with the supplied fields.
    ///
    /// This constructor uses the provided version and stores all the
    /// provided values. No network encoding is performed at this stage.
    #[must_use]
    pub const fn new(
        version: u8,
        marker: u16,
        payload_type: u8,
        payload: Vec<u8>,
        timestamp: u32,
        frame_id: u64,
        chunk_id: u64,
        ssrc: u32,
    ) -> Self {
        Self {
            version,
            marker,
            payload_type,
            frame_id,
            chunk_id,
            timestamp,
            ssrc,
            payload,
        }
    }

    /// Encode the packet into a sequence of bytes suitable for sending
    /// over the network.
    ///
    /// The layout used here is a simple custom header followed by the
    /// payload. The method allocates a buffer and appends fields in
    /// network byte order.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(26 + self.payload.len());

        buf.push(self.version);
        buf.push(self.payload_type);
        buf.extend_from_slice(&self.frame_id.to_be_bytes());
        buf.extend_from_slice(&self.chunk_id.to_be_bytes());
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.extend_from_slice(&(self.marker).to_be_bytes());
        buf.extend_from_slice(&self.ssrc.to_be_bytes());
        buf.extend_from_slice(&self.payload);

        buf
    }

    /// Decode an `RtpPacket` from a byte slice previously produced by
    /// `to_bytes`. Returns `None` if the slice is too short or malformed.
    #[must_use]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 28 {
            return None;
        }

        let version = data[0];
        let payload_type = data[1];
        let frame_id = u64::from_be_bytes(array_from_slice::<8>(&data[2..10]));
        let chunk_id = u64::from_be_bytes(array_from_slice::<8>(&data[10..18]));
        let timestamp = u32::from_be_bytes(array_from_slice::<4>(&data[18..22]));
        let total_chunks = u16::from_be_bytes(array_from_slice::<2>(&data[22..24]));
        let ssrc = u32::from_be_bytes(array_from_slice::<4>(&data[24..28]));
        let payload = data[28..].to_vec();

        Some(Self {
            version,
            payload_type,
            frame_id,
            chunk_id,
            timestamp,
            marker: total_chunks,
            ssrc,
            payload,
        })
    }
}

/// Helper: copy `N` bytes from the slice into a fixed-size array.
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
        // Create a packet with unique, non-zero values for all fields
        let original_packet = RtpPacket::new(
            2,                        // version
            5,                        // marker
            96,                       // payload_type
            vec![10, 20, 30, 40, 50], // payload
            1_122_334_455,            // timestamp
            123_456_789,              // frame_id
            42,                       // chunk_id
            987_654_321,              // ssrc
        );

        // Serialize and then deserialize the packet
        let bytes = original_packet.to_bytes();
        let deserialized_option = RtpPacket::from_bytes(&bytes);

        assert!(deserialized_option.is_some(), "from_bytes returned None");

        let deserialized_packet = deserialized_option.unwrap();

        // Assert every single field is identical
        assert_eq!(
            original_packet.version, deserialized_packet.version,
            "Version mismatch"
        );
        assert_eq!(
            original_packet.marker, deserialized_packet.marker,
            "Marker mismatch"
        );
        assert_eq!(
            original_packet.payload_type, deserialized_packet.payload_type,
            "Payload type mismatch"
        );
        assert_eq!(
            original_packet.frame_id, deserialized_packet.frame_id,
            "Frame ID mismatch"
        );
        assert_eq!(
            original_packet.chunk_id, deserialized_packet.chunk_id,
            "Chunk ID mismatch"
        );
        assert_eq!(
            original_packet.timestamp, deserialized_packet.timestamp,
            "Timestamp mismatch"
        );
        assert_eq!(
            original_packet.ssrc, deserialized_packet.ssrc,
            "SSRC mismatch"
        );
        assert_eq!(
            original_packet.payload, deserialized_packet.payload,
            "Payload data mismatch"
        );
    }

    #[test]
    fn test_from_bytes_too_short() {
        // Header is 28 bytes, we send 27
        let short_bytes: &[u8] = &[0; 27];

        let result = RtpPacket::from_bytes(short_bytes);

        assert!(
            result.is_none(),
            "Expected None for a packet shorter than the header"
        );
    }

    #[test]
    fn test_from_bytes_exactly_header_size() {
        // A packet with 28 bytes should deserialize into a packet with an empty payload
        let header_only_bytes: &[u8] = &[
            2,  // version
            96, // payload_type
            0, 0, 0, 0, 0, 0, 0, 1, // frame_id = 1
            0, 0, 0, 0, 0, 0, 0, 0, // chunk_id = 0
            0, 0, 0, 100, // timestamp = 100
            0, 1, // marker = 1
            0, 0, 0, 2, // ssrc = 2
        ];

        let result = RtpPacket::from_bytes(header_only_bytes);

        assert!(
            result.is_some(),
            "Packet with exact header size should deserialize"
        );
        let packet = result.unwrap();
        assert_eq!(packet.frame_id, 1);
        assert_eq!(packet.timestamp, 100);
        assert_eq!(packet.ssrc, 2);
        assert!(packet.payload.is_empty(), "Payload should be empty");
    }

    #[test]
    fn test_from_bytes_empty_slice() {
        let empty_bytes: &[u8] = &[];

        let result = RtpPacket::from_bytes(empty_bytes);

        assert!(result.is_none(), "Expected None for an empty byte slice");
    }
}
