#[derive(Debug, Clone)]

pub struct RtpPacket {
    version: u8,
    marker: bool,
    pub(crate) payload_type: u8,
    pub frame_id: u64,
    pub chunk_id: u64,
    pub timestamp: u32,
    pub(crate) ssrc: u32,
    pub payload: Vec<u8>,
}

impl RtpPacket {
    pub fn new(
        marker: bool,
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

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(12 + self.payload.len());

        let b0 = (self.version << 6) | ((self.marker as u8) << 7);
        buf.push(b0);
        buf.push(self.payload_type);

        buf.extend_from_slice(&self.frame_id.to_be_bytes());
        buf.extend_from_slice(&self.chunk_id.to_be_bytes());
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.extend_from_slice(&self.ssrc.to_be_bytes());

        buf.extend_from_slice(&self.payload);

        buf
    }

    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 12 {
            return None;
        }

        let version = data[0] >> 6;
        let marker = (data[0] & 0x80) != 0;
        let payload_type = data[1];
        let frame_id = u64::from_be_bytes(array_from_slice::<8>(&data[2..10]));
        let chunk_id = u64::from_be_bytes(array_from_slice::<8>(&data[10..18]));
        let timestamp = u32::from_be_bytes(array_from_slice::<4>(&data[18..22]));
        let ssrc = u32::from_be_bytes(array_from_slice::<4>(&data[22..26]));
        let payload = data[26..].to_vec();

        Some(Self {
            version,
            marker,
            payload_type,
            frame_id,
            chunk_id,
            timestamp,
            ssrc,
            payload,
        })
    }
}

fn array_from_slice<const N: usize>(slice: &[u8]) -> [u8; N] {
    let mut arr = [0u8; N];
    arr.copy_from_slice(&slice[..N]);
    arr
}
