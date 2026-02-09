use crate::config::Config;
use crate::controller::AppEvent;
use crate::dtls::dtls_socket::DtlsSocket;
use crate::dtls::key_manager::LocalCert;
use crate::file::{
    file_metadata::FileMetadata,
    file_transferer::{FileTransfererError as Error, ftp_message::FTPMessage},
};
use crate::logger::Logger;
use crate::sctp_transport::SCTPTransport;
use crate::sctp_transport::data_channel::{DataChannel, DataChannelType};
use crate::session::sdp::{DtlsSetupRole, Fingerprint};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, UdpSocket};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const FILE_CHUNK_SIZE: usize = 512;

#[derive(Clone)]
pub struct FileTransferer {
    transport: SCTPTransport,
    pending_data_channels: Arc<Mutex<HashMap<u32, DataChannel>>>,
    next_offer_id: u32,
    connected: Arc<AtomicBool>,
    event_tx: Sender<AppEvent>,
    logger: Arc<Logger>,
}

impl FileTransferer {
    pub fn new(
        local_addres: SocketAddr,
        peer_address: SocketAddr,
        local_setup_role: DtlsSetupRole,
        expected_fingerprint: Fingerprint,
        local_cert: &LocalCert,
        event_tx: Sender<AppEvent>,
        connected: Arc<AtomicBool>,
        logger: Arc<Logger>,
        config: Arc<Config>,
    ) -> Result<Self, Error> {
        let is_client = matches!(local_setup_role, DtlsSetupRole::Active);
        let sctp_socket =
            UdpSocket::bind(local_addres).map_err(|e| Error::MapError(e.to_string()))?;

        let socket = DtlsSocket::new(
            sctp_socket,
            peer_address,
            local_setup_role,
            expected_fingerprint,
            local_cert,
        )
        .map_err(|e| Error::MapError(e.to_string()))?;

        let mut transport = SCTPTransport::new(connected.clone(), logger.clone(), config.clone());
        transport
            .connect(peer_address, socket, is_client)
            .map_err(|e| Error::MapError(e.to_string()))?;

        let instance = Self {
            transport,
            pending_data_channels: Arc::new(Mutex::new(HashMap::new())),
            next_offer_id: 0,
            connected,
            event_tx,
            logger,
        };

        instance.wait_for_incoming();
        Ok(instance)
    }

    pub fn send_file(&mut self, file_path: &Path) -> Result<(), Error> {
        let mut file = File::open(file_path).map_err(|e| Error::MapError(e.to_string()))?;
        let file_metadata =
            FileMetadata::from(file_path, &file).map_err(|e| Error::MapError(e.to_string()))?;

        let mut data_channel = self
            .transport
            .open_data_channel(
                file_metadata.name.clone(),
                DataChannelType::Reliable,
                0,
                "".to_string(),
            )
            .map_err(|e| Error::MapError(e.to_string()))?;

        let offer_id = self.next_offer_id;
        let logger = self.logger.clone();
        let connected = self.connected.clone();
        self.next_offer_id += 1;

        thread::spawn(move || {
            match file_offer_accepted(offer_id, &mut data_channel, file_metadata) {
                Ok(true) => {
                    let mut message: Vec<u8>;
                    let mut buff = [0u8; FILE_CHUNK_SIZE];
                    while connected.load(Ordering::SeqCst) {
                        let n = match file
                            .read(&mut buff)
                            .map_err(|e| Error::FileReadError(e.to_string()))
                        {
                            Ok(n) => n,
                            Err(e) => {
                                logger.error(e.to_string().as_str());
                                break;
                            }
                        };
                        if n == 0 {
                            break;
                        }

                        message = FTPMessage::FileChunk {
                            payload: buff[..n].to_vec(),
                        }
                        .to_bytes();

                        if let Err(e) = data_channel.send(&message) {
                            logger.error(e.to_string().as_str());
                            break;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                    message = FTPMessage::EndOfFile.to_bytes();
                    if let Err(e) = data_channel.send(&message) {
                        logger.error(e.to_string().as_str());
                    }
                }
                Ok(false) => {
                    // Offer rejected by remote; nothing to do.
                }
                Err(e) => {
                    logger.error(e.to_string().as_str());
                }
            }
        });

        Ok(())
    }

    pub fn reject_file_offer(&mut self, offer_id: u32) -> Result<(), Error> {
        let mut dc = self
            .pending_data_channels
            .lock()
            .map_err(|e| Error::LockError(e.to_string()))?
            .remove(&offer_id)
            .ok_or(Error::UnknownOfferId(offer_id))?;
        dc.send(&FTPMessage::RejectFile.to_bytes()).map_err(|e| {
            self.logger.error(e.to_string().as_str());
            Error::MapError(e.to_string())
        })
    }

    pub fn accept_file_offer(&mut self, offer_id: u32, download_path: &Path) -> Result<(), Error> {
        let mut dc = self
            .pending_data_channels
            .lock()
            .map_err(|e| Error::LockError(e.to_string()))?
            .remove(&offer_id)
            .ok_or(Error::UnknownOfferId(offer_id))?;
        dc.send(&FTPMessage::AcceptFile.to_bytes())
            .map_err(|e| Error::MapError(e.to_string()))?;
        let file =
            File::create(download_path).map_err(|e| Error::FileCreateError(e.to_string()))?;
        Self::recv_file(
            offer_id,
            dc,
            file,
            self.connected.clone(),
            self.event_tx.clone(),
            self.logger.clone(),
        );
        Ok(())
    }

    fn recv_file(
        offer_id: u32,
        mut dc: DataChannel,
        mut file: File,
        connected: Arc<AtomicBool>,
        event_tx: Sender<AppEvent>,
        logger: Arc<Logger>,
    ) {
        thread::spawn(move || {
            let mut buff = [0u8; FILE_CHUNK_SIZE + 5];
            while connected.load(Ordering::SeqCst) {
                let n = match dc.recv(&mut buff) {
                    Ok(n) => n,
                    Err(e) => {
                        logger.error(e.to_string().as_str());
                        break;
                    }
                };

                if n == 0 {
                    break;
                }
                match FTPMessage::from_bytes(&buff[..n]) {
                    Some(FTPMessage::FileChunk { payload }) => {
                        if let Err(e) = file.write_all(&payload) {
                            logger.error(Error::FileWriteError(e.to_string()).to_string().as_str());
                            break;
                        }
                    }
                    Some(FTPMessage::EndOfFile) => {
                        if let Err(e) = event_tx.send(AppEvent::FileDownloadCompleted(offer_id)) {
                            logger
                                .error(Error::ChannelSendError(e.to_string()).to_string().as_str());
                        }
                        break;
                    }
                    Some(_) => logger.warn(Error::UnexpectedIncomingMessage.to_string().as_str()),
                    None => logger.warn(Error::UnknownIncomingMessage.to_string().as_str()),
                }
            }
        });
    }

    fn wait_for_incoming(&self) {
        let mut instance = self.clone();

        thread::spawn(move || {
            while instance.connected.load(Ordering::SeqCst) {
                let dc = match instance.transport.accept_data_channel() {
                    Ok(dc) => dc,
                    Err(e) => {
                        instance
                            .logger
                            .warn(Error::MapError(e.to_string()).to_string().as_str());
                        continue;
                    }
                };
                if let Err(e) = instance.handle_incoming(dc, instance.event_tx.clone()) {
                    instance.logger.error(e.to_string().as_str());
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        });
    }

    fn handle_incoming(
        &mut self,
        mut data_channel: DataChannel,
        event_tx: Sender<AppEvent>,
    ) -> Result<(), Error> {
        let mut buff = vec![0u8; FILE_CHUNK_SIZE + 5];
        loop {
            let n = data_channel
                .recv(&mut buff)
                .map_err(|e| Error::FileReadError(e.to_string()))?;
            if let Some(FTPMessage::FileOffer {
                offer_id,
                file_metadata,
            }) = FTPMessage::from_bytes(&buff[..n])
            {
                self.pending_data_channels
                    .lock()
                    .map_err(|e| Error::LockError(e.to_string()))?
                    .insert(offer_id, data_channel);
                event_tx
                    .send(AppEvent::RemoteFileOffer(offer_id, file_metadata))
                    .map_err(|e| Error::ChannelSendError(e.to_string()))?;

                return Ok(());
            }
        }
    }
}

fn file_offer_accepted(
    offer_id: u32,
    data_channel: &mut impl DataChannelLike,
    file_metadata: FileMetadata,
) -> Result<bool, Error> {
    let message = FTPMessage::FileOffer {
        offer_id,
        file_metadata,
    }
    .to_bytes();

    data_channel
        .send(&message)
        .map_err(|e| Error::ChannelSendError(e.to_string()))?;

    let mut buff = vec![0u8; 1024];
    loop {
        let n = data_channel
            .recv(&mut buff)
            .map_err(|e| Error::MapError(e.to_string()))?;
        match FTPMessage::from_bytes(&buff[..n]) {
            Some(FTPMessage::RejectFile) => return Ok(false),
            Some(FTPMessage::AcceptFile) => return Ok(true),
            _ => {}
        }
    }
}

// Abstraction to allow testing with a fake data channel.
pub trait DataChannelLike {
    fn send(&mut self, message: &[u8]) -> Result<(), Error>;
    fn recv(&mut self, buff: &mut [u8]) -> Result<usize, Error>;
}

impl DataChannelLike for DataChannel {
    fn send(&mut self, message: &[u8]) -> Result<(), Error> {
        self.send(message)
            .map_err(|e| Error::ChannelSendError(e.to_string()))
    }

    fn recv(&mut self, buff: &mut [u8]) -> Result<usize, Error> {
        self.recv(buff).map_err(|e| Error::MapError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::file_metadata::FileMetadata;

    struct FakeDataChannel {
        pub sent: Vec<Vec<u8>>,
        pub responses: std::sync::Mutex<Vec<Vec<u8>>>,
    }

    impl FakeDataChannel {
        fn new(responses: Vec<Vec<u8>>) -> Self {
            Self {
                sent: vec![],
                responses: std::sync::Mutex::new(responses),
            }
        }
    }

    impl DataChannelLike for FakeDataChannel {
        fn send(&mut self, message: &[u8]) -> Result<(), Error> {
            self.sent.push(message.to_vec());
            Ok(())
        }

        fn recv(&mut self, buff: &mut [u8]) -> Result<usize, Error> {
            let mut guard = match self.responses.lock() {
                Ok(g) => g,
                Err(_) => return Err(Error::MapError("mutex poisoned".to_string())),
            };
            if guard.is_empty() {
                return Ok(0);
            }
            let data = guard.remove(0);
            let n = data.len().min(buff.len());
            buff[..n].copy_from_slice(&data[..n]);
            Ok(n)
        }
    }

    #[test]
    fn file_offer_accepted_returns_true_on_accept() {
        let meta = FileMetadata {
            size: 42,
            name: "file.bin".to_string(),
        };

        let mut fake = FakeDataChannel::new(vec![FTPMessage::AcceptFile.to_bytes()]);

        let res = file_offer_accepted(7, &mut fake, meta.clone()).expect("call should succeed");
        assert!(res);
        // verify that a FileOffer was sent
        assert_eq!(fake.sent.len(), 1);
        let sent = &fake.sent[0];
        if let Some(FTPMessage::FileOffer {
            offer_id,
            file_metadata,
        }) = FTPMessage::from_bytes(sent)
        {
            assert_eq!(offer_id, 7);
            assert_eq!(file_metadata.name, meta.name);
            assert_eq!(file_metadata.size, meta.size);
        } else {
            panic!("sent message was not FileOffer");
        }
    }

    #[test]
    fn file_offer_accepted_returns_false_on_reject() {
        let meta = FileMetadata {
            size: 10,
            name: "a.txt".to_string(),
        };

        let mut fake = FakeDataChannel::new(vec![FTPMessage::RejectFile.to_bytes()]);

        let res = file_offer_accepted(99, &mut fake, meta).expect("call should succeed");
        assert!(!res);
    }
}
