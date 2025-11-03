use crate::tools::socket::Socket;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct MockSocket {
    pub(crate) data_to_receive: Vec<Vec<u8>>,
    pub(crate) sent_data: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl Socket for MockSocket {
    fn send(&self, buf: &[u8]) -> Result<usize, std::io::Error> {
        self.sent_data
            .lock()
            .map_err(|_| {
                std::io::Error::other(
                    "Failed to acquire lock due to poisoning",
                )
            })?
            .push(buf.to_vec());
        Ok(buf.len())
    }

    fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, std::net::SocketAddr), std::io::Error> {
        if let Some(data) = self.data_to_receive.first() {
            let len = data.len().min(buf.len());
            buf[..len].copy_from_slice(&data[..len]);
            Ok((
                len,
                "127.0.0.1:1234".parse().map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Failed to parse socket address",
                    )
                })?,
            ))
        } else {
            Err(std::io::ErrorKind::WouldBlock.into())
        }
    }

    fn set_read_timeout(&self, _dur: Option<Duration>) -> Result<(), std::io::Error> {
        Ok(())
    }
}
