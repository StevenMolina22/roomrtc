use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtp::error::RtpError as Error;
use crate::transport::rtp::rtp_packet::RtpPacket;
use crate::tools::Socket;
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::Duration;
use crate::config::Config;

/// RTP receiver that reads `RtpPacket` instances from a socket and
/// manages RTCP reporting through a `RtcpReportHandler`.
///
/// This type owns a socket implementing the project's `Socket` trait,
/// a locked `RtcpReportHandler` used to drive RTCP-style reporting and
/// a shared `connection_status` used to observe/drive session state.
pub struct RtpReceiver<S: Socket + Send + Sync + 'static> {
    config: Arc<Config>,
    rtp_socket: S,
    report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
    connected: Arc<AtomicBool>,
}

impl<S: Socket + Send + Sync + 'static> RtpReceiver<S> {
    /// Create a new `RtpReceiver`.
    ///
    /// # Parameters
    /// - `rtp_socket`: socket bound to the local RTP port implementing
    ///   the `Socket` trait.
    /// - `report_handler`: an `Arc<Mutex<RtcpReportHandler<S>>>` used to
    ///   control and close RTCP reporting when needed.
    /// - `connection_status`: shared `Arc<RwLock<ConnectionStatus>>`
    ///   representing the current session state. The receiver uses this
    ///   value to detect when the session has been closed.
    ///
    /// # Errors
    /// Returns `Error::SocketConfigFailed` if configuring the socket
    /// read timeout fails.
    pub fn new(
        config: &Arc<Config>,
        rtp_socket: S,
        report_handler: &Arc<Mutex<RtcpReportHandler<S>>>,
        connected: &Arc<AtomicBool>,
    ) -> Result<Self, Error> {
        rtp_socket
            .set_read_timeout(Some(Duration::from_millis(config.rtp.read_timeout_millis)))
            .map_err(|_| Error::SocketConfigFailed)?;

        Ok(Self {
            config: Arc::clone(config),
            rtp_socket,
            report_handler: Arc::clone(report_handler),
            connected: Arc::clone(connected),
        })
    }

    pub fn start(&mut self) -> Result<Receiver<RtpPacket>, Error> {
        let (remote_to_local_rtp_tx, remote_to_local_rtp_rx) = mpsc::channel();
        
        let connected = self.connected.clone();
        let rtp_socket = self.rtp_socket.try_clone().map_err(|_| Error::SocketCloneFailed)?;
        let rtcp_handler = self.report_handler.clone();
        let config = self.config.clone();
        
        thread::spawn({
            move || {
                loop {
                    if !connected.load(Ordering::SeqCst) {
                        println!("RTP receiver thread disconnected");
                        break;
                    }
                    let rtp_packet = match receive(&rtp_socket, &connected, &config,&rtcp_handler) {
                        Ok(packet) => packet,
                        Err(e) => {
                            println!("RTP receiver receive failed: {e}");
                            break;
                        }
                    };

                    if let Err(e) = remote_to_local_rtp_tx.send(rtp_packet) {
                        println!("RTP receiver send to channel failed: {e}");
                        break;
                    }
                }
            }
        });

        Ok(remote_to_local_rtp_rx)
    }

}
/// Wait for and return the next decoded `RtpPacket`.
///
/// This method loops until a valid `RtpPacket` is decoded from the
/// underlying socket. If the shared `connection_status` transitions
/// to `Closed` while waiting, the method returns
/// `Error::ConnectionClosed`. On other socket errors it will attempt
/// to close the RTCP reporting handler and propagate a
/// `ReceiveFailed` error.
///
/// # Errors
/// Returns `Error::ReceiveFailed` for unexpected socket errors and
/// `Error::ConnectionClosed` if the session is closed while waiting.
pub fn receive<S: Socket + Send + Sync + 'static>(rtp_socket: &S, connected: &Arc<AtomicBool>, config: &Arc<Config>, report_handler: &Arc<Mutex<RtcpReportHandler<S>>>) -> Result<RtpPacket, Error> {
    let mut buf = vec![0u8; 65535];
    
    loop {
        match rtp_socket.recv_from(&mut buf) {
            Ok((size, _addr)) => {
                if let Some(packet) = RtpPacket::from_bytes(&buf[..size]) {
                    return Ok(packet);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if !connected.load(Ordering::SeqCst) {
                    return Err(Error::ConnectionClosed);
                }
            }
            Err(e) => {
                report_handler
                    .lock()
                    .map_err(|_| Error::PoisonedLock)?
                    .close_connection()
                    .map_err(|e| Error::RTCPError(e.to_string()))?;
                return Err(Error::ReceiveFailed(e.to_string()));
            }
        }
        //buf.clear();
    }
}