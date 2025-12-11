use crate::config::Config;
use crate::controller::AppEvent;
use crate::logger::Logger;
use crate::tools::Socket;
use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtp::error::RtpError as Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

/// RTP receiver that reads encrypted RTP packets from a socket.
///
/// The receiver wraps a socket implementing the project's `Socket` trait
/// and coordinates with an RTCP report handler for session management.
/// It spawns a background thread that continuously reads encrypted packet
/// data from the socket and forwards it to a channel for decryption.
///
/// # Threading Model
/// The receiver operates in a background thread with a configured read timeout,
/// allowing the application to receive packets asynchronously via a channel.
pub struct RtpReceiver<S: Socket + Send + Sync + 'static> {
    /// Socket for receiving RTP packets from the remote peer.
    rtp_socket: S,
    /// RTCP report handler for session management and goodbye signaling.
    report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
    /// Shared connection status flag coordinating thread lifecycle.
    connected: Arc<AtomicBool>,
    /// Logger instance for receiver-specific messages.
    logger: Logger,
}

impl<S: Socket + Send + Sync + 'static> RtpReceiver<S> {
    /// Create a new `RtpReceiver` with the provided socket and dependencies.
    ///
    /// Configures the socket with a read timeout from the application config
    /// and initializes the receiver with references to the RTCP handler and
    /// connection status. Does not start the background thread; call `start()`
    /// to begin reception.
    ///
    /// # Parameters
    /// - `config`: application configuration (contains RTP read timeout).
    /// - `rtp_socket`: socket for receiving RTP packets.
    /// - `report_handler`: RTCP report handler for session coordination.
    /// - `connected`: shared connection status flag.
    /// - `logger`: logger instance for this receiver.
    ///
    /// # Returns
    /// A configured `RtpReceiver` ready to `start()`.
    ///
    /// # Errors
    /// Returns `Error::SocketConfigFailed` if setting the read timeout fails.
    pub fn new(
        config: &Arc<Config>,
        rtp_socket: S,
        report_handler: &Arc<Mutex<RtcpReportHandler<S>>>,
        connected: &Arc<AtomicBool>,
        logger: Logger,
    ) -> Result<Self, Error> {
        rtp_socket
            .set_read_timeout(Some(Duration::from_millis(config.rtp.read_timeout_millis)))
            .map_err(|_| Error::SocketConfigFailed)?;

        Ok(Self {
            rtp_socket,
            report_handler: Arc::clone(report_handler),
            connected: Arc::clone(connected),
            logger,
        })
    }

    /// Start the RTP receiver by spawning a background reception thread.
    ///
    /// Creates a channel for forwarding received encrypted packet data and spawns
    /// a thread that continuously reads from the socket and sends data through the
    /// channel. The thread runs until the connection is closed or an error occurs.
    /// If the thread terminates due to error while the connection is still active,
    /// it sends a `CallEnded` event to notify the application.
    ///
    /// # Parameters
    /// - `event_tx`: channel for sending application events (call end notifications).
    ///
    /// # Returns
    /// A receiver channel for encrypted RTP packet data (Vec<u8>).
    ///
    /// # Errors
    /// Returns `Error::SocketCloneFailed` if cloning the socket for the thread fails.
    pub fn start(&mut self, event_tx: Sender<AppEvent>) -> Result<Receiver<Vec<u8>>, Error> {
        let (remote_to_local_rtp_tx, remote_to_local_rtp_rx) = mpsc::channel();

        let connected = self.connected.clone();
        let rtp_socket = self
            .rtp_socket
            .try_clone()
            .map_err(|_| Error::SocketCloneFailed)?;

        let rtcp_handler = self.report_handler.clone();
        let logger = self.logger.clone();

        thread::spawn({
            move || {
                loop {
                    if !connected.load(Ordering::SeqCst) {
                        break;
                    }
                    let protected_data = match receive(&rtp_socket, &connected, &rtcp_handler) {
                        Ok(protected_data) => protected_data,
                        Err(e) => {
                            logger.error(&format!("RtpReceiver error: {e}"));
                            break;
                        }
                    };

                    if let Err(e) = remote_to_local_rtp_tx.send(protected_data) {
                        logger.error(&format!("Failed to send received packet to channel: {e}"));
                        break;
                    }
                }

                if connected.load(Ordering::SeqCst) {
                    connected.store(false, Ordering::SeqCst);
                    let _ = event_tx.send(AppEvent::CallEnded);
                }
                logger.info("RtpReceiver thread terminated");
            }
        });

        Ok(remote_to_local_rtp_rx)
    }
}

/// Wait for and receive the next encrypted RTP packet from the socket.
///
/// This function loops until a complete packet is received from the socket.
/// It handles `WouldBlock` errors by checking the connection status and
/// continuing to wait. On other errors, it attempts to send an RTCP goodbye
/// before returning the error.
///
/// # Parameters
/// - `rtp_socket`: the socket to receive data from.
/// - `connected`: connection status flag to check during timeout waits.
/// - `report_handler`: RTCP handler for sending goodbye on fatal errors.
///
/// # Returns
/// A byte vector containing the encrypted packet data.
///
/// # Errors
/// - `Error::ConnectionClosed` if the connection flag is set to false during a timeout.
/// - `Error::ReceiveFailed` if a fatal socket error occurs.
pub fn receive<S: Socket + Send + Sync + 'static>(
    rtp_socket: &S,
    connected: &Arc<AtomicBool>,
    report_handler: &Arc<Mutex<RtcpReportHandler<S>>>,
) -> Result<Vec<u8>, Error> {
    let mut buf = vec![0u8; 65535];
    loop {
        match rtp_socket.recv_from(&mut buf) {
            Ok((size, _addr)) => {
                return Ok(buf[0..size].to_vec());
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
                    .report_goodbye()
                    .map_err(|e| Error::RTCPError(e.to_string()))?;
                return Err(Error::ReceiveFailed(e.to_string()));
            }
        }
    }
}
