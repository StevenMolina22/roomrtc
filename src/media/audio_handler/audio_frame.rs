pub struct AudioFrame {
    pub data: Vec<f32>,
    pub timestamp: u128
}

impl AudioFrame {
    pub fn new(data: Vec<f32>, timestamp: u128) -> Self {
        Self { data, timestamp }
    }
}