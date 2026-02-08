use crate::config::Config;
use crate::controller::AppEvent;
use crate::file::{
    file_metadata::FileMetadata,
    file_transferer::{FileTransfererError as Error, ftp_message::FTPMessage},
};
use crate::sctp_transport::SCTPTransport;
use crate::sctp_transport::data_channel::{DataChannel, DataChannelType};
use crate::session::sdp::{DtlsSetupRole, Fingerprint};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, UdpSocket};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use crate::dtls::dtls_socket::DtlsSocket;
use crate::dtls::key_manager::LocalCert;
use crate::logger::Logger;

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
        let sctp_socket = UdpSocket::bind(local_addres).unwrap();

        let socket = DtlsSocket::new(
            sctp_socket,
            peer_address,
            local_setup_role,
            expected_fingerprint,
            local_cert,
        ).map_err(|e| Error::MapError(e.to_string()))?;

        let mut transport = SCTPTransport::new(connected.clone(), logger.clone(), config.clone());
        transport.connect(peer_address, socket, is_client).map_err(|e| Error::MapError(e.to_string()))?;

        let instance = Self {
            transport,
            pending_data_channels: Arc::new(Mutex::new(HashMap::new())),
            next_offer_id: 0,
            connected,
            event_tx,
            logger
        };

        instance.wait_for_incoming();
        Ok(instance)
    }

    pub fn send_file(&mut self, file_path: &Path) -> Result<(), Error> {
        let mut file = File::open(file_path).unwrap();
        let file_metadata = FileMetadata::from(&file_path, &file).unwrap();

        let mut data_channel = self
            .transport
            .open_data_channel(
                file_metadata.name.clone(),
                DataChannelType::Reliable,
                0,
                "".to_string(), // QUE PONGO ACA EN PROTOCOL??? FTP??
            )
            .map_err(|e| Error::MapError(e.to_string()))?;
        
        let offer_id = self.next_offer_id;
        let logger = self.logger.clone();
        let connected = self.connected.clone();
        self.next_offer_id += 1;

        thread::spawn(move || {
            if file_offer_accepted(offer_id, &mut data_channel, file_metadata).unwrap() {
                let mut message: Vec<u8>;
                let mut buff = [0u8; FILE_CHUNK_SIZE];
                while connected.load(Ordering::SeqCst) {
                    let n = match file.read(&mut buff).map_err(|e| Error::FileReadError(e.to_string())) {
                        Ok(n) => n,
                        Err(e) => {
                            logger.error(e.to_string().as_str());
                            break
                        }
                    };
                    if n == 0 {
                        break
                    }

                    message = FTPMessage::FileChunk {
                        payload: buff[..n].to_vec(),
                    }
                    .to_bytes();

                    if let Err(e) = data_channel.send(&message) {
                        logger.error(e.to_string().as_str());
                        break
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                message = FTPMessage::EndOfFile.to_bytes();
                if let Err(e) = data_channel.send(&message) {
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
            .unwrap()
            .remove(&offer_id)
            .unwrap();
        dc.send(&FTPMessage::RejectFile.to_bytes()).map_err(|e| {
            self.logger.error(e.to_string().as_str());
            Error::MapError(e.to_string())
        })
    }

    pub fn accept_file_offer(&mut self, offer_id: u32, download_path: &Path) -> Result<(), Error> {
        let mut dc = self
            .pending_data_channels
            .lock()
            .unwrap()
            .remove(&offer_id)
            .unwrap();
        dc.send(&FTPMessage::AcceptFile.to_bytes()).map_err(|e| Error::MapError(e.to_string()))?;
        let file = File::create(download_path).map_err(|e| Error::FileCreateError(e.to_string()))?;
        Self::recv_file(offer_id, dc, file, self.connected.clone(), self.event_tx.clone(), self.logger.clone());
        Ok(())
    }

    fn recv_file(offer_id: u32, mut dc: DataChannel, mut file: File, connected: Arc<AtomicBool>, event_tx: Sender<AppEvent>, logger: Arc<Logger>) {
        thread::spawn(move || {
            let mut buff = [0u8; FILE_CHUNK_SIZE + 5];
            while connected.load(Ordering::SeqCst) {
                let n = match dc.recv(&mut buff){
                    Ok(n) => n,
                    Err(e) => {
                        logger.error(e.to_string().as_str());
                        break
                    },
                };

                if n == 0 {
                    break;
                }
                match FTPMessage::from_bytes(&buff[..n]) {
                    Some(FTPMessage::FileChunk { payload }) => if let Err(e) = file.write_all(&payload) {
                            logger.error(Error::FileWriteError(e.to_string()).to_string().as_str());
                            break
                        },
                    Some(FTPMessage::EndOfFile) => {
                        if let Err(e) = event_tx.send(AppEvent::FileDownloadCompleted(offer_id)) {
                            logger.error(Error::ChannelSendError(e.to_string()).to_string().as_str());
                        }
                        break
                    },
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
                        instance.logger.warn(Error::MapError(e.to_string()).to_string().as_str());
                        continue
                    }
                };
                if let Err(e) = instance.handle_incoming(dc, instance.event_tx.clone()) {
                    instance.logger.error(e.to_string().as_str());
                    break
                }
                thread::sleep(Duration::from_millis(20));
            }
        });
    }

    fn handle_incoming(&mut self, mut data_channel: DataChannel, event_tx: Sender<AppEvent>) -> Result<(), Error> {
        let mut buff = vec![0u8; FILE_CHUNK_SIZE + 5];
        loop {
            let n = data_channel.recv(&mut buff).map_err(|e| Error::FileReadError(e.to_string()))?;
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
    data_channel: &mut DataChannel,
    file_metadata: FileMetadata,
) -> Result<bool, Error> {
    let message = FTPMessage::FileOffer {
        offer_id,
        file_metadata,
    }
    .to_bytes();

    data_channel.send(&message).map_err(|e| Error::ChannelSendError(e.to_string()))?;

    let mut buff = vec![0u8; 1024];
    loop {
        let n = data_channel.recv(&mut buff).map_err(|e| Error::MapError(e.to_string()))?;
        match FTPMessage::from_bytes(&buff[..n]) {
            Some(FTPMessage::RejectFile) => {
                return Ok(false)
            },
            Some(FTPMessage::AcceptFile) => {
                return Ok(true)
            },
            _ => {},
        }
    }
}
