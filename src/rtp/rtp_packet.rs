#[derive(Debug, Clone)]

pub struct RtpPacket {
    version: u8,
    pub marker: u16,
    pub(crate) payload_type: u8,
    pub frame_id: u64,
    pub chunk_id: u64,
    pub timestamp: u32,
    pub(crate) ssrc: u32,
    pub payload: Vec<u8>,
}

impl RtpPacket {
    pub fn new(
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

fn array_from_slice<const N: usize>(slice: &[u8]) -> [u8; N] {
    let mut arr = [0u8; N];
    arr.copy_from_slice(&slice[..N]);
    arr
}
