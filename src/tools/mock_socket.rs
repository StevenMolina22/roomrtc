use crate::tools::socket::Socket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
pub struct MockSocket {
    pub(crate) data_to_receive: Arc<Mutex<Vec<Vec<u8>>>>,
    pub(crate) sent_data: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl MockSocket {
    #[must_use]
    pub fn new(data: Vec<Vec<u8>>) -> Self {
        Self {
            data_to_receive: Arc::new(Mutex::new(data)),
            sent_data: Arc::new(Mutex::new(vec![])),
        }
    }
}

impl Socket for MockSocket {
    fn send(&self, buf: &[u8]) -> Result<usize, std::io::Error> {
        self.sent_data
            .lock()
            .map_err(|_| std::io::Error::other("sent_data poisoned"))?
            .push(buf.to_vec());

        Ok(buf.len())
    }

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

        Ok((len, "127.0.0.1:1234".parse().unwrap()))
    }

    fn set_read_timeout(&self, _dur: Option<Duration>) -> Result<(), std::io::Error> {
        Ok(())
    }

    fn try_clone(&self) -> Result<Self, std::io::Error> {
        // Clona el socket compartiendo el mismo estado interno
        Ok(self.clone())
    }
}
