use crate::transport::MediaTransportError as Error;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, Sender};
use crate::config::Config;
use crate::tools::Socket;
use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtp::{RtpPacket, RtpReceiver, RtpSender};

pub struct MediaTransport {
    config: Arc<Config>,

    pub rtp_address: SocketAddr,
    rtp_socket: UdpSocket,

    rtcp_socket: UdpSocket,
    rtcp_handler: Option<Arc<Mutex<RtcpReportHandler<UdpSocket>>>>,

    connected: Arc<AtomicBool>,
}

impl MediaTransport {
    pub fn new(config: &Arc<Config>) -> Result<Self, Error> {
        let rtp_socket = UdpSocket::bind(format!("{}:0", config.network.bind_address))
            .map_err(|e| Error::BindingError(e.to_string()))?;

        let rtp_address = rtp_socket
            .local_addr()
            .map_err(|e| Error::BindingError(e.to_string()))?;

        let rtcp_address = SocketAddr::new(rtp_address.ip(), rtp_address.port() + 1);
        let rtcp_socket = UdpSocket::bind(rtcp_address)
            .map_err(|e| Error::BindingError(e.to_string()))?;


        Ok(Self {
            config: Arc::clone(config),
            rtp_address,
            rtp_socket,
            rtcp_socket,
            rtcp_handler: None,
            connected: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn start(&mut self, remote_rtp_address: SocketAddr, remote_rtcp_address: SocketAddr) -> Result<(Sender<RtpPacket>, Receiver<RtpPacket>), Error> {
        self.rtp_socket.connect(remote_rtp_address).map_err(|e| Error::SocketConnectionError(e.to_string()))?;
        self.rtcp_socket.connect(remote_rtcp_address).map_err(|e| Error::SocketConnectionError(e.to_string()))?;

        println!("RTP binded adrr: {}", self.rtp_socket.local_addr().unwrap());
        println!("RTP connected adrr: {}\n", remote_rtp_address);
        println!("RTCP binded adrr: {}", self.rtcp_socket.local_addr().unwrap());
        println!("RTCP connected adrr: {}\n", remote_rtcp_address);

        let rtcp_handler = RtcpReportHandler::new(
            self.rtcp_socket.try_clone().map_err(|e| Error::CloningSocketError(e.to_string()))?,
            Arc::clone(&self.connected),
            self.config.rtcp.clone(),
        );

        rtcp_handler.start().map_err(|e| Error::MapError(e.to_string()))?;
        self.rtcp_handler = Some(Arc::new(Mutex::new(rtcp_handler)));

        self.spawn_rtp_threads()
    }

    pub fn stop(&mut self) -> Result<(), Error> {
        if let Some(rtcp_handler) = &self.rtcp_handler {
            if let Ok(rtcp_handler) = rtcp_handler.lock() {
                return rtcp_handler.close_connection().map_err(|e| Error::MapError(e.to_string()))
            }
        }

        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }


    fn spawn_rtp_threads(&self) -> Result<(Sender<RtpPacket>, Receiver<RtpPacket>), Error> {
        let rtcp_handler = match &self.rtcp_handler {
            Some(handler_lock) => Arc::clone(handler_lock),
            None => return Err(Error::ConnectionNotStarted),
        };

        let local_to_remote_rtp_tx = self.start_rtp_sender(&rtcp_handler)?;
        let remote_to_local_rtp_rx = self.start_rtp_receiver(&rtcp_handler)?;

        Ok((local_to_remote_rtp_tx, remote_to_local_rtp_rx))
    }

    fn start_rtp_sender(&self, rtcp_handler: &Arc<Mutex<RtcpReportHandler<UdpSocket>>>) -> Result<Sender<RtpPacket>, Error> {
        let rtp_sender_socket = self.rtp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;

        let rtp_sender = RtpSender::new(
            rtp_sender_socket,
            rtcp_handler,
            self.config.media.default_ssrc,
            &self.connected,
            self.config.media.rtp_version,
        ).map_err(|e| Error::MapError(e.to_string()))?;

        rtp_sender.start().map_err(|e| Error::MapError(e.to_string()))
    }

    fn start_rtp_receiver(&self, rtcp_handler: &Arc<Mutex<RtcpReportHandler<UdpSocket>>>) -> Result<Receiver<RtpPacket>, Error> {
        let rtp_receiver_socket = self.rtp_socket
            .try_clone()
            .map_err(|e| Error::CloningSocketError(e.to_string()))?;

        let mut rtp_receiver = RtpReceiver::new(
            &self.config,
            rtp_receiver_socket,
            rtcp_handler,
            &self.connected,
        )
            .map_err(|e| Error::MapError(e.to_string()))?;

        rtp_receiver.start().map_err(|e| Error::MapError(e.to_string()))
    }
}