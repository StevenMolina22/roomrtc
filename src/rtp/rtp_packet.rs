use std::default::Default;

/// RTP packet used by the project transport layer.
///
/// This struct models a simplified RTP-like packet used on top of the
/// project's UDP sockets. The packet uses a custom binary layout
/// that packs a small header followed by payload
/// bytes.
#[derive(Debug, Clone)]
#[derive(Default)]
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
    /// This constructor sets `version` to 2 and stores the provided
    /// values. No network encoding is performed at this stage.
    #[must_use] 
    pub const fn new(
        marker: u16,
        payload_type: u8,
        payload: Vec<u8>,
        timestamp: u32,
        frame_id: u64,
        chunk_id: u64,
        ssrc: u32,
    ) -> Self {
        Self {
            version: 2,
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
