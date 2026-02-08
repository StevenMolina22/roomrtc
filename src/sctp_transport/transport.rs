use crate::config::Config;
use crate::dtls::DtlsSocket;
use crate::logger::Logger;
use crate::sctp_transport::SCTPTransportError as Error;
use crate::sctp_transport::data_channel::DataChannel;
use crate::sctp_transport::data_channel::DataChannelType;
use crate::tools::Socket;
use bytes::Bytes;
use sctp_proto::{
    Association, AssociationHandle, ClientConfig, DatagramEvent, Endpoint, EndpointConfig, Event,
    Payload, PayloadProtocolIdentifier, ServerConfig, StreamEvent, StreamId, Transmit,
};
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct SCTPTransport {
    endpoint: Arc<Mutex<Endpoint>>,
    association: Arc<Mutex<Association>>,
    association_handle: Arc<RwLock<Option<AssociationHandle>>>,
    next_stream_id: StreamId,
    connected: Arc<AtomicBool>,
    logger: Arc<Logger>,
    config: Arc<Config>,
}

impl SCTPTransport {
    pub fn new(connected: Arc<AtomicBool>, logger: Arc<Logger>, config: Arc<Config>) -> Self {
        let endpoint = Arc::new(Mutex::new(Endpoint::new(
            Arc::new(EndpointConfig::default()),
            Some(Arc::new(ServerConfig::default())),
        )));

        Self {
            endpoint,
            association: Arc::new(Mutex::new(Association::default())),
            association_handle: Arc::new(RwLock::new(None)),
            next_stream_id: 0,
            connected,
            logger,
            config,
        }
    }

    pub fn connect(
        &mut self,
        peer_address: SocketAddr,
        socket: DtlsSocket,
        is_client: bool,
    ) -> Result<(), Error> {
        if is_client {
            let (association_handle, association) = self
                .endpoint
                .lock()
                .map_err(|e| Error::PoisonedLock(e.to_string()))?
                .connect(ClientConfig::default(), peer_address)
                .map_err(|e| Error::ConnectError(e.to_string()))?;

            self.association_handle
                .write()
                .map_err(|e| Error::PoisonedLock(e.to_string()))?
                .replace(association_handle);

            self.association = Arc::new(Mutex::new(association));
            self.next_stream_id = 1;
        }

        socket
            .set_read_timeout(Some(Duration::from_millis(20)))
            .map_err(|e| Error::SocketConfigError(e.to_string()))?;
        self.start_event_loop(peer_address, socket);

        while self
            .association
            .lock()
            .map_err(|e| Error::PoisonedLock(e.to_string()))?
            .is_handshaking()
        {
            thread::sleep(Duration::from_millis(0));
        }
        Ok(())
    }

    pub fn open_data_channel(
        &mut self,
        label: String,
        dc_type: DataChannelType,
        reliability_param: u32,
        protocol: String,
    ) -> Result<DataChannel, Error> {
        self.association
            .lock()
            .map_err(|e| Error::PoisonedLock(e.to_string()))?
            .open_stream(self.next_stream_id, PayloadProtocolIdentifier::Binary)
            .map_err(|e| Error::OpenStreamError(e.to_string()))?;

        let dc = DataChannel::open(
            label,
            self.next_stream_id,
            self.association.clone(),
            dc_type,
            reliability_param,
            protocol,
            Arc::new(self.logger.context("DataChannel")),
            self.config.clone(),
        )
        .map_err(|e| Error::OpenDataChannelError(e.to_string()))?;
        self.next_stream_id += 2;
        Ok(dc)
    }

    pub fn accept_data_channel(&self) -> Result<DataChannel, Error> {
        while self.connected.load(Ordering::SeqCst) {
            let stream_id = {
                self.association
                    .lock()
                    .map_err(|e| Error::PoisonedLock(e.to_string()))?
                    .accept_stream()
                    .map(|s| s.stream_identifier())
            };

            if let Some(id) = stream_id {
                return DataChannel::from_accepted_stream(
                    id,
                    self.association.clone(),
                    Arc::new(self.logger.context("DataChannel")),
                    self.config.clone(),
                )
                .map_err(|e| Error::MapError(e.to_string()));
            }

            thread::sleep(Duration::from_millis(500));
        }
        Err(Error::ConnectError("".to_string()))
    }

    fn start_event_loop(&mut self, peer_address: SocketAddr, socket: DtlsSocket) {
        let mut instance = self.clone();

        thread::spawn(move || {
            let mut buf = vec![0u8; 1024];

            while instance.connected.load(Ordering::SeqCst) {
                let now = Instant::now();

                match socket.recv_from(&mut buf) {
                    Ok((len, _addr)) => {
                        if instance.handle_incoming(peer_address, &buf, len).is_err() {}
                    }
                    Err(e)
                        if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                    }
                    Err(_) => break, //loggear
                }

                if match instance.association_handle.read() {
                    Ok(assoc_handle) => assoc_handle.is_some(),
                    Err(_) => break, //loggear
                } {
                    {
                        let mut assoc = match instance.association.lock() {
                            Ok(assoc) => assoc,
                            Err(_) => break, //loggear
                        };

                        if let Some(time) = assoc.poll_timeout()
                            && now >= time
                        {
                            assoc.handle_timeout(now);
                        }

                        while let Some(endpoint_event) = assoc.poll_endpoint_event() {
                            let assoc_handle = match instance.association_handle.read() {
                                Ok(assoc_handle) => assoc_handle,
                                Err(_) => return, //loggear
                            };
                            let a = match *assoc_handle {
                                Some(a) => a,
                                None => return, //loggear
                            };
                            match instance.endpoint.lock() {
                                Ok(mut endpoint) => endpoint.handle_event(a, endpoint_event),
                                Err(_) => return, //loggear
                            };
                        }
                    }

                    if instance.handle_association_events().is_err() {
                        //loggear
                        return;
                    }

                    loop {
                        let mut assoc = match instance.association.lock() {
                            Ok(assoc) => assoc,
                            Err(_) => return, //logger
                        };
                        match assoc.poll_transmit(Instant::now()) {
                            Some(transmit) => {
                                if send_transmit(transmit, &socket).is_err() {
                                    return; //loggear
                                }
                            }
                            None => break,
                        }
                    }

                    loop {
                        let mut endpoint = match instance.endpoint.lock() {
                            Ok(endpoint) => endpoint,
                            Err(_) => return, //loggear
                        };

                        match endpoint.poll_transmit() {
                            Some(transmit) => {
                                if send_transmit(transmit, &socket).is_err() {
                                    return; //loggear
                                }
                            }
                            None => break,
                        }
                    }
                }
                thread::sleep(Duration::from_millis(0));
            }
        });
    }

    fn handle_incoming(
        &mut self,
        remote: SocketAddr,
        data: &[u8],
        len: usize,
    ) -> Result<(), Error> {
        if let Some((assoc_handle, datagram_event)) = self
            .endpoint
            .lock()
            .map_err(|e| Error::PoisonedLock(e.to_string()))?
            .handle(
                Instant::now(),
                remote,
                None,
                None,
                Bytes::copy_from_slice(&data[..len]),
            )
        {
            match datagram_event {
                DatagramEvent::AssociationEvent(e) => self
                    .association
                    .lock()
                    .map_err(|e| Error::PoisonedLock(e.to_string()))?
                    .handle_event(e),
                DatagramEvent::NewAssociation(a) => {
                    if self
                        .association_handle
                        .read()
                        .map_err(|e| Error::PoisonedLock(e.to_string()))?
                        .is_none()
                    {
                        {
                            let mut shared_association = self
                                .association
                                .lock()
                                .map_err(|e| Error::PoisonedLock(e.to_string()))?;
                            *shared_association = a;
                        }
                        {
                            let mut shared_association_handle = self
                                .association_handle
                                .write()
                                .map_err(|e| Error::PoisonedLock(e.to_string()))?;
                            *shared_association_handle = Some(assoc_handle);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_association_events(&mut self) -> Result<(), Error> {
        loop {
            let event = match self
                .association
                .lock()
                .map_err(|e| Error::PoisonedLock(e.to_string()))?
                .poll()
            {
                Some(event) => event,
                None => return Ok(()),
            };

            self.handle_event(event);
        }
    }

    //loggear individualmente cada caso
    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Stream(stream_event) => match stream_event {
                StreamEvent::Opened => {}
                StreamEvent::BufferedAmountLow { id: _ } => {}
                StreamEvent::Readable { id: _ } => {}
                StreamEvent::Finished { id: _ } => {}
                StreamEvent::Writable { .. } => {}
                StreamEvent::Stopped { .. } => {}
                StreamEvent::Available => {}
            },
            Event::Connected => {}
            Event::AssociationLost { reason: _ } => {}
            Event::DatagramReceived => {}
        }
    }
}

fn send_transmit(transmit: Transmit, socket: &impl Socket) -> Result<(), Error> {
    if let Payload::RawEncode(chunks) = transmit.payload {
        for chunk in chunks {
            socket
                .send(&chunk)
                .map_err(|e| Error::IOError(e.to_string()))?;
        }
    };
    Ok(())
}
