pub struct AudioFrame {
    pub data: Vec<f32>,
    pub timestamp: u128,
}

impl AudioFrame {
    pub const fn new(data: Vec<f32>, timestamp: u128) -> Self {
        Self { data, timestamp }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_frame_creation() {
        let data = vec![0.0, 0.5, -0.5];
        let timestamp = 123_456_789;
        let frame = AudioFrame::new(data.clone(), timestamp);
        assert_eq!(frame.data, data);
        assert_eq!(frame.timestamp, timestamp);
    }
}
