use crate::logger::Logger;
use crate::tools::Socket;
use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtcp::metrics::SenderStats;
use crate::transport::rtp::error::RtpError as Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

/// RTP sender that transmits encrypted RTP packets over a socket.
///
/// The sender wraps a socket implementing the project's `Socket` trait
/// and coordinates with an RTCP report handler for session management.
/// It spawns a background thread that receives encrypted packet data from
/// a channel and transmits it to the remote peer while updating sender
/// statistics for RTCP reporting.
///
/// # Threading Model
/// The sender operates in a background thread, allowing the application
/// to send packets asynchronously via a channel without blocking.
pub struct RtpSender<S: Socket + Send + Sync + 'static> {
    /// Socket for sending RTP packets to the remote peer.
    rtp_socket: S,
    /// RTCP report handler for session management and goodbye signaling.
    report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
    /// Shared connection status flag coordinating thread lifecycle.
    connected: Arc<AtomicBool>,
    /// Shared sender statistics for RTCP reporting.
    metrics: Arc<Mutex<SenderStats>>,
    /// Logger instance for sender-specific messages.
    logger: Logger,
}

impl<S: Socket + Send + Sync + 'static> RtpSender<S> {
    /// Create a new `RtpSender` with the provided socket and dependencies.
    ///
    /// Initializes the sender with references to the RTCP handler, connection
    /// status flag, and sender statistics. Does not start the background thread;
    /// call `start()` to begin transmission.
    ///
    /// # Parameters
    /// - `rtp_socket`: socket for sending RTP packets.
    /// - `report_handler`: RTCP report handler for session coordination.
    /// - `connected`: shared connection status flag.
    /// - `metrics`: shared sender statistics.
    /// - `logger`: logger instance for this sender.
    ///
    /// # Returns
    /// A configured `RtpSender` ready to `start()`.
    pub fn new(
        rtp_socket: S,
        report_handler: &Arc<Mutex<RtcpReportHandler<S>>>,
        connected: &Arc<AtomicBool>,
        metrics: Arc<Mutex<SenderStats>>,
        logger: Logger,
    ) -> Result<Self, Error> {
        Ok(Self {
            rtp_socket,
            report_handler: Arc::clone(report_handler),
            connected: Arc::clone(connected),
            metrics,
            logger,
        })
    }

    /// Start the RTP sender by spawning a background transmission thread.
    ///
    /// Creates a channel for receiving encrypted packet data and spawns a thread
    /// that continuously reads from the channel and transmits packets over the
    /// socket. The thread runs until the connection is closed or an error occurs.
    ///
    /// # Returns
    /// A sender channel for encrypted RTP packet data (Vec<u8>).
    ///
    /// # Errors
    /// Returns `Error::SocketCloneFailed` if cloning the socket for the thread fails.
    pub fn start(&self) -> Result<Sender<Vec<u8>>, Error> {
        let (tx, rx) = mpsc::channel();

        let rtp_socket = self
            .rtp_socket
            .try_clone()
            .map_err(|_| Error::SocketCloneFailed)?;
        let report_handler = Arc::clone(&self.report_handler);
        let connected = Arc::clone(&self.connected);
        let metrics = Arc::clone(&self.metrics);
        let logger = self.logger.clone();

        thread::spawn(move || {
            loop {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                let protected_data = match rx.recv() {
                    Ok(p) => p,
                    Err(_) => {
                        break;
                    }
                };

                if let Err(e) = send_packet(
                    &rtp_socket,
                    &report_handler,
                    &connected,
                    protected_data,
                    &metrics,
                ) {
                    logger.error(&format!("RtpSender error: {e}"));
                    break;
                }
            }
        });

        Ok(tx)
    }
}

/// Send an encrypted RTP packet over the socket and update statistics.
///
/// This function checks the connection status, transmits the packet data,
/// and updates sender metrics (packets sent, bytes sent). If the send fails,
/// it attempts to notify the peer via RTCP goodbye before returning an error.
///
/// # Parameters
/// - `socket`: the socket to send data through.
/// - `report_handler`: RTCP handler for sending goodbye on error.
/// - `connected`: connection status flag to check before sending.
/// - `protected_packet`: encrypted packet data to transmit.
/// - `metrics`: sender statistics to update after successful transmission.
///
/// # Returns
/// `Ok(())` on successful transmission, otherwise an error.
///
/// # Errors
/// - `Error::ConnectionClosed` if the connection is already closed.
/// - `Error::SendFailed` if socket transmission fails.
fn send_packet<S: Socket + Send + Sync + 'static>(
    socket: &S,
    report_handler: &Arc<Mutex<RtcpReportHandler<S>>>,
    connected: &AtomicBool,
    protected_packet: Vec<u8>,
    metrics: &Arc<Mutex<SenderStats>>,
) -> Result<(), Error> {
    if !connected.load(Ordering::SeqCst) {
        return Err(Error::ConnectionClosed);
    }

    if socket.send(&protected_packet).is_err() {
        report_handler
            .lock()
            .map_err(|_| Error::PoisonedLock)?
            .report_goodbye()
            .map_err(|e| Error::RTCPError(e.to_string()))?;
        return Err(Error::SendFailed);
    }

    if let Ok(mut m) = metrics.lock() {
        m.packets_sent += 1;
        m.bytes_sent += protected_packet.len() as u64;
    }
    Ok(())
}
