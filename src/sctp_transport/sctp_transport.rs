use std::io::ErrorKind;
use crate::config::Config;
use crate::sctp_transport::SCTPTransportError as Error;
use crate::sctp_transport::data_channel::DataChannel;
use crate::sctp_transport::data_channel::DataChannelType;
use crate::tools::Socket;
use bytes::Bytes;
use sctp_proto::{Association, AssociationHandle, ClientConfig, DatagramEvent, Endpoint, EndpointConfig, Event, Payload, PayloadProtocolIdentifier, ServerConfig, StreamEvent, StreamId, Transmit};
use std::net::{SocketAddr};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use crate::dtls::DtlsSocket;

#[derive(Clone)]
pub struct SCTPTransport {
    endpoint: Arc<Mutex<Endpoint>>,
    association: Arc<Mutex<Association>>,
    association_handle: Arc<RwLock<Option<AssociationHandle>>>,
    next_stream_id: StreamId,
    config: Arc<Config>,
}

impl SCTPTransport {
    pub fn new(config: Arc<Config>) -> Self {
        let endpoint = Arc::new(Mutex::new(Endpoint::new(
            Arc::new(EndpointConfig::default()),
            Some(Arc::new(ServerConfig::default())),
        )));

        Self {
            endpoint,
            association: Arc::new(Mutex::new(Association::default())),
            association_handle: Arc::new(RwLock::new(None)),
            next_stream_id: 0,
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
                .unwrap()
                .connect(ClientConfig::default(), peer_address)
                .unwrap();

            self.association_handle
                .write()
                .unwrap()
                .replace(association_handle);
            self.association = Arc::new(Mutex::new(association));
            self.next_stream_id = 1;
        }

        socket
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        self.start_event_loop(peer_address, socket);

        while self.association.lock().unwrap().is_handshaking() {
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
            .unwrap()
            .open_stream(self.next_stream_id, PayloadProtocolIdentifier::Binary)
            .unwrap();

        let dc = DataChannel::open(
            label,
            self.next_stream_id,
            self.association.clone(),
            dc_type,
            reliability_param,
            protocol,
            self.config.clone(),
        )
        .map_err(|e| Error::OpenDataChannelError(e.to_string()))?;
        self.next_stream_id += 2;

        Ok(dc)
    }

    pub fn accept_data_channel(&self) -> Result<DataChannel, Error> {
        loop {
            let stream_id = {
                self.association.lock().unwrap().accept_stream().and_then(|s| Some(s.stream_identifier()))
            };

            if let Some(id) = stream_id {
                return Ok(DataChannel::from_accepted_stream(
                    id,
                    self.association.clone(),
                    self.config.clone(),
                ).unwrap())
            }

            thread::sleep(Duration::from_millis(500));
        }
    }

    fn start_event_loop(&mut self, peer_address: SocketAddr, socket: DtlsSocket) {
        let mut instance = self.clone();

        thread::spawn(move || {
            let mut buf = vec![0u8; 1024];

            loop {
                let now = Instant::now();

                match socket.recv_from(&mut buf) {
                    Ok((len, _addr)) => instance.handle_incoming(peer_address, &buf, len),
                    Err(e) if e.kind() == ErrorKind::WouldBlock
                        || e.kind() == ErrorKind::TimedOut => {},
                    Err(e) => panic!("recv_from failed: {e}"),
                }

                if instance.association_handle.read().unwrap().is_some() {
                    {
                        let mut assoc = instance.association.lock().unwrap();
                        if let Some(time) = assoc.poll_timeout()
                            && now >= time
                        {
                            assoc.handle_timeout(now);
                        }

                        while let Some(endpoint_event) = assoc.poll_endpoint_event() {
                            let assoc_handle = instance.association_handle.read().unwrap().unwrap();
                            instance
                                .endpoint
                                .lock()
                                .unwrap()
                                .handle_event(assoc_handle, endpoint_event);
                        }
                    }

                    instance.handle_association_events().unwrap();

                    while let Some(transmit) = instance
                        .association
                        .lock()
                        .unwrap()
                        .poll_transmit(Instant::now())
                    {
                        send_transmit(transmit, &socket).unwrap();
                    }
                    while let Some(transmit) = instance.endpoint.lock().unwrap().poll_transmit() {
                        send_transmit(transmit, &socket).unwrap();
                    }
                }
                thread::sleep(Duration::from_millis(0));
            }
        });
    }

    fn handle_incoming(&mut self, remote: SocketAddr, data: &[u8], len: usize) {
        if let Some((assoc_handle, datagram_event)) = self.endpoint.lock().unwrap().handle(
            Instant::now(),
            remote,
            None,
            None,
            Bytes::copy_from_slice(&data[..len]),
        ) {
            match datagram_event {
                DatagramEvent::AssociationEvent(e) => {
                    self.association.lock().unwrap().handle_event(e)
                }
                DatagramEvent::NewAssociation(a) => {
                    if self.association_handle.read().unwrap().is_none() {
                        {
                            let mut shared_association = self.association.lock().unwrap();
                            *shared_association = a;
                        }
                        {
                            let mut shared_association_handle = self.association_handle.write().unwrap();
                            *shared_association_handle = Some(assoc_handle);
                        }
                    }
                }
            }
        }
    }

    fn handle_association_events(&mut self) -> Result<(), Error> {
        loop {
            let event = match self.association.lock().unwrap().poll() {
                Some(event) => event,
                None => return Ok(()),
            };

            self.handle_event(event)?;
        }
    }
    fn handle_event(&mut self, event: Event) -> Result<(), Error> {
        match event {
            Event::Stream(stream_event) => {
                match stream_event {
                    StreamEvent::Opened => {}
                    StreamEvent::BufferedAmountLow { id: _ } => {}
                    StreamEvent::Readable { id: _ } => {}
                    StreamEvent::Finished { id: _ } => {}
                    StreamEvent::Writable { .. } => {}
                    StreamEvent::Stopped { .. } => {}
                    StreamEvent::Available => {}
                }
            }
            Event::Connected => {}
            Event::AssociationLost { reason: _ } => {}
            Event::DatagramReceived => {}
        }
        Ok(())
    }
}

fn send_transmit(transmit: Transmit, socket: &impl Socket) -> Result<(), Error> {
    match transmit.payload {
        Payload::RawEncode(chunks) => {
            for chunk in chunks {
                socket
                    .send(&chunk)
                    .map_err(|e| Error::IOError(e.to_string()))?;
            }
        }
        Payload::PartialDecode(_) => {}
    };
    Ok(())
}
