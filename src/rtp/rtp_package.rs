pub struct RtpPackage {
    version: u8,
    marker: bool,
    payload_type: u8,
    sequence_number: u16,
    timestamp: u32,
    ssrc: u32,
    payload: Vec<u8>,
}

impl RtpPackage {
    pub fn new(marker: bool, payload_type: u8, payload: Vec<u8>, timestamp: u32, sequence_number: u16, ssrc: u32) -> Self {
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
}

