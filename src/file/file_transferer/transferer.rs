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
use std::thread;
use std::time::Duration;
use crate::dtls::dtls_socket::DtlsSocket;
use crate::dtls::key_manager::LocalCert;

const FILE_CHUNK_SIZE: usize = 512;

#[derive(Clone)]
pub struct FileTransferer {
    transport: SCTPTransport,
    pending_data_channels: Arc<Mutex<HashMap<u32, DataChannel>>>,
    next_offer_id: u32,
    event_tx: Sender<AppEvent>,
}

impl FileTransferer {
    pub fn new(
        local_addres: SocketAddr,
        peer_address: SocketAddr,
        local_setup_role: DtlsSetupRole,
        expected_fingerprint: Fingerprint,
        local_cert: &LocalCert,
        event_tx: Sender<AppEvent>,
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
        ).unwrap();

        let mut transport = SCTPTransport::new(config.clone());
        transport.connect(peer_address, socket, is_client).unwrap();

        let instance = Self {
            transport,
            pending_data_channels: Arc::new(Mutex::new(HashMap::new())),
            next_offer_id: 0,
            event_tx,
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
            .unwrap();
        
        let offer_id = self.next_offer_id;
        self.next_offer_id += 1;
        thread::spawn(move || {
            if file_offer_accepted(offer_id, &mut data_channel, file_metadata).unwrap() {
                let mut message: Vec<u8>;
                let mut buff = [0u8; FILE_CHUNK_SIZE];
                loop {
                    let n = file.read(&mut buff).unwrap();
                    if n == 0 {
                        break;
                    }

                    message = FTPMessage::FileChunk {
                        payload: buff[..n].to_vec(),
                    }
                    .to_bytes();

                    data_channel.send(&message).unwrap();
                    thread::sleep(Duration::from_millis(50));
                }
                message = FTPMessage::EndOfFile.to_bytes();
                data_channel.send(&message).unwrap();
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
        dc.send(&FTPMessage::RejectFile.to_bytes()).unwrap();
        Ok(())
    }

    pub fn accept_file_offer(&mut self, offer_id: u32, download_path: &Path) -> Result<(), Error> {
        let mut dc = self
            .pending_data_channels
            .lock()
            .unwrap()
            .remove(&offer_id)
            .unwrap();
        dc.send(&FTPMessage::AcceptFile.to_bytes()).unwrap();
        let file = File::create(download_path).unwrap();
        Self::recv_file(offer_id, dc, file, self.event_tx.clone())?;
        Ok(())
    }

    fn recv_file(offer_id: u32, mut dc: DataChannel, mut file: File, event_tx: Sender<AppEvent>) -> Result<(), Error> {
        thread::spawn(move || {
            let mut buff = [0u8; FILE_CHUNK_SIZE + 5];
            loop {
                let n = dc.recv(&mut buff).unwrap();
                if n == 0 {
                    break;
                }
                match FTPMessage::from_bytes(&buff[..n]).unwrap() {
                    FTPMessage::FileChunk { payload } => {
                        file.write_all(&payload).unwrap()
                    },
                    FTPMessage::EndOfFile => {
                        event_tx.send(AppEvent::FileDownloadCompleted(offer_id)).unwrap();
                        break
                    },
                    _ => panic!("wrong message type"),
                }
            }
        });
        Ok(())
    }

    fn wait_for_incoming(&self) {
        let mut instance = self.clone();
        
        thread::spawn(move || {
            loop {
                let dc = instance.transport.accept_data_channel().unwrap();
                instance.handle_incoming(dc, instance.event_tx.clone());
                thread::sleep(Duration::from_millis(20));
            }
        });
    }

    fn handle_incoming(&mut self, mut data_channel: DataChannel, event_tx: Sender<AppEvent>) {
        let mut buff = vec![0u8; FILE_CHUNK_SIZE + 5];
        loop {
            let n = data_channel.recv(&mut buff).unwrap();
            if let Some(FTPMessage::FileOffer {
                offer_id,
                file_metadata,
            }) = FTPMessage::from_bytes(&buff[..n])
            {
                self.pending_data_channels
                    .lock()
                    .unwrap()
                    .insert(offer_id, data_channel);
                event_tx
                    .send(AppEvent::RemoteFileOffer(offer_id, file_metadata))
                    .unwrap();
                break;
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

    data_channel.send(&message).unwrap();

    let mut buff = vec![0u8; 1024];
    loop {
        let n = data_channel.recv(&mut buff).unwrap();
        match FTPMessage::from_bytes(&buff[..n]) {
            Some(FTPMessage::RejectFile) => {
                return Ok(false)
            }, // ACA TENDRIA Q ENVIAR UN MENSAJE POR EL CANAL ANTES DE TERMINAR EL HILO
            Some(FTPMessage::AcceptFile) => {
                return Ok(true)
            },
            _ => {},
        }
    }
}
