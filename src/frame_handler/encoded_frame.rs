pub struct EncodedFrame {
    pub id: u64,
    pub chunks: Vec<Vec<u8>>,
    pub width: usize,
    pub height: usize,
}