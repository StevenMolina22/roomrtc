use crate::tools::socket::Socket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A mock socket implementation for testing purposes.
///
/// This struct simulates socket operations by maintaining internal buffers
/// for sent and received data, allowing controlled testing of network behavior.
#[derive(Clone)]
pub struct MockSocket {
    /// Buffer containing data that will be returned by `recv_from` operations.
    pub(crate) data_to_receive: Arc<Mutex<Vec<Vec<u8>>>>,
    /// Buffer storing data that was sent through `send` operations.
    pub(crate) sent_data: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl MockSocket {
    /// Creates a new mock socket with pre-defined data to receive.
    ///
    /// # Arguments
    /// * `data` - A vector of byte vectors representing packets to be received.
    ///
    /// # Returns
    /// A new `MockSocket` instance.
    #[must_use]
    pub fn new(data: Vec<Vec<u8>>) -> Self {
        Self {
            data_to_receive: Arc::new(Mutex::new(data)),
            sent_data: Arc::new(Mutex::new(vec![])),
        }
    }
}

impl Socket for MockSocket {
    /// Stores the sent data in the internal buffer.
    fn send(&self, buf: &[u8]) -> Result<usize, std::io::Error> {
        self.sent_data
            .lock()
            .map_err(|_| std::io::Error::other("sent_data poisoned"))?
            .push(buf.to_vec());

        Ok(buf.len())
    }

    /// Retrieves the next packet from the mock buffer and copies it to the provided buffer.
    fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, std::net::SocketAddr), std::io::Error> {
        let mut data = self
            .data_to_receive
            .lock()
            .map_err(|_| std::io::Error::other("data_to_receive poisoned"))?;

        if data.is_empty() {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }

        // Consume el primer paquete
        let packet = data.remove(0);

        let len = packet.len().min(buf.len());
        buf[..len].copy_from_slice(&packet[..len]);

        let addr = "127.0.0.1:1234"
            .parse()
            .map_err(|e| std::io::Error::other(format!("invalid mock socket addr: {}", e)))?;

        Ok((len, addr))
    }

    /// Mock implementation that does nothing (timeout not simulated).
    fn set_read_timeout(&self, _dur: Option<Duration>) -> Result<(), std::io::Error> {
        Ok(())
    }

    /// Clones the socket sharing the same internal state.
    fn try_clone(&self) -> Result<Self, std::io::Error> {
        Ok(self.clone())
    }
}
