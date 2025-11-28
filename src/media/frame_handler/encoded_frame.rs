use std::fmt::{Display, Formatter};

/// A container for an encoded video frame.
///
/// `EncodedFrame` holds an identifier plus a list of data chunks that
/// together represent the compressed frame bytes. Width and height are
/// the intended decoded dimensions and are stored for convenience by
/// consumers of the frame.
pub struct EncodedFrame {
    /// Unique identifier for the frame.
    pub id: u64,

    /// The compressed frame split into one or more chunks.
    pub chunks: Vec<Vec<u8>>,

    /// Frame width in pixels (decoded size).
    pub width: usize,

    /// Frame height in pixels (decoded size).
    pub height: usize,
}

impl Display for EncodedFrame {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // cantidad de bytes a mostrar como preview
        let preview_len = 16;

        // tamaños de cada chunk
        let chunk_sizes: Vec<usize> =
            self.chunks.iter().map(|c| c.len()).collect();

        // preview del primer chunk (si existe)
        let preview = if let Some(first_chunk) = self.chunks.first() {
            let hex_bytes: Vec<String> = first_chunk
                .iter()
                .take(preview_len)
                .map(|b| format!("{:02X}", b))
                .collect();

            format!("[{}]{}", hex_bytes.join(" "),
                    if first_chunk.len() > preview_len { " ..." } else { "" })
        } else {
            "[]".to_string()
        };

        write!(
            f,
            "EncodedFrame {{ id: {}, size: {}x{}, chunks: {}, chunk_sizes: {:?}, first_chunk_preview: {} }}",
            self.id,
            self.width,
            self.height,
            self.chunks.len(),
            chunk_sizes,
            preview
        )
    }
}
