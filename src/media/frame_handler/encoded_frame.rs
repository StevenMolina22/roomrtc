/// A container for an encoded video frame.
///
/// `EncodedFrame` holds an identifier plus a list of data chunks that
/// together represent the compressed frame bytes. Width and height are
/// the intended decoded dimensions and are stored for convenience by
/// consumers of the frame.
pub struct EncodedFrame {
    /// The compressed frame split into one or more chunks.
    pub chunks: Vec<Vec<u8>>,
    
    /// Frame time
    pub frame_time: u128,

    /// Asserts if the encoded frame is Intra
    pub is_i_frame: bool,
}
