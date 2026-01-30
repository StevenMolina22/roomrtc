/// A circular buffer for audio sample storage.
/// 
/// Provides efficient FIFO storage for audio samples with automatic wraparound.
/// When the buffer is full, new data overwrites the oldest samples.
/// 
/// # Use Cases
/// 
/// - Buffering audio samples between resampling and frame extraction
/// - Handling variable-sized input chunks while producing fixed-size output frames
/// - Managing audio data flow in real-time processing pipelines
pub struct AudioRingBuffer {
    buffer: Vec<f32>,
    size: usize,
    write_pos: usize,
    read_pos: usize,
    count: usize,
}

impl AudioRingBuffer {
    /// Creates a new ring buffer with the specified capacity.
    /// 
    /// # Arguments
    /// 
    /// * `capacity` - Maximum number of f32 samples the buffer can hold
    /// 
    /// # Returns
    /// 
    /// A new `AudioRingBuffer` initialized with all samples set to 0.0
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0.0; capacity],
            size: capacity,
            write_pos: 0,
            read_pos: 0,
            count: 0,
        }
    }

    /// Adds audio samples to the buffer.
    /// 
    /// If the buffer becomes full, the oldest samples are overwritten.
    /// The read position automatically advances to maintain data coherence.
    /// 
    /// # Arguments
    /// 
    /// * `data` - Slice of audio samples to add to the buffer
    pub fn push(&mut self, data: &[f32]) {
        for &sample in data {
            self.buffer[self.write_pos] = sample;
            self.write_pos = (self.write_pos + 1) % self.size;
            
            if self.count < self.size {
                self.count += 1;
            } else {
                // Buffer lleno: Avanzamos el lector para mantener la coherencia
                self.read_pos = (self.read_pos + 1) % self.size;
            }
        }
    }

    /// Attempts to extract an exact block of samples from the buffer.
    /// 
    /// # Arguments
    /// 
    /// * `chunk_size` - Number of samples to extract
    /// 
    /// # Returns
    /// 
    /// - `Some(Vec<f32>)` containing exactly `chunk_size` samples if available
    /// - `None` if the buffer contains fewer than `chunk_size` samples
    /// 
    /// # Note
    /// 
    /// Samples are removed from the buffer in FIFO order.
    pub fn pop_chunk(&mut self, chunk_size: usize) -> Option<Vec<f32>> {
        if self.count < chunk_size {
            return None;
        }

        let mut chunk = Vec::with_capacity(chunk_size);
        for _ in 0..chunk_size {
            chunk.push(self.buffer[self.read_pos]);
            self.read_pos = (self.read_pos + 1) % self.size;
        }
        self.count -= chunk_size;
        Some(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_flow() {
        let mut rb = AudioRingBuffer::new(10);
        
        rb.push(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        
        let chunk1 = rb.pop_chunk(2).unwrap();
        assert_eq!(chunk1, vec![1.0, 2.0]);
        
        let chunk2 = rb.pop_chunk(2).unwrap();
        assert_eq!(chunk2, vec![3.0, 4.0]);
        
        assert!(rb.pop_chunk(2).is_none());
    }

    #[test]
    fn test_buffer_wrap_around() {
        let mut rb = AudioRingBuffer::new(4);
        rb.push(&[1.0, 2.0, 3.0]); 
        let _ = rb.pop_chunk(2); 
        
        rb.push(&[4.0, 5.0]); 
        
        let chunk = rb.pop_chunk(3).unwrap();
        assert_eq!(chunk, vec![3.0, 4.0, 5.0]);
    }
}