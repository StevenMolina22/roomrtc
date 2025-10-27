#[derive(Debug, Clone)]

pub struct RtpPackage {
    version: u8,
    marker: bool,
    payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    ssrc: u32,
    pub payload: Vec<u8>,
}

impl RtpPackage {
    pub fn new(
        marker: bool,
        payload_type: u8,
        payload: Vec<u8>,
        timestamp: u32,
        sequence_number: u16,
        ssrc: u32,
    ) -> Self {
        Self {
            version: 2,
            marker,
            payload_type,
            sequence_number,
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

        buf.extend_from_slice(&self.sequence_number.to_be_bytes());
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
        let sequence_number = u16::from_be_bytes([data[2], data[3]]);
        let timestamp = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ssrc = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let payload = data[12..].to_vec();

        Some(Self {
            version,
            marker,
            payload_type,
            sequence_number,
            timestamp,
            ssrc,
            payload,
        })
    }
}
