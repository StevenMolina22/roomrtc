use crate::tools::Socket;
use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtp::error::RtpError as Error;
use crate::transport::rtp::rtp_packet::RtpPacket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use crate::controller::AppEvent;

/// RTP sender that transmits `RtpPacket`s and manages RTCP reporting.
///
/// The sender wraps a socket implementing the project's `Socket` trait
/// and an RTCP report handler. It offers `send` for sending payloads as
/// `RtpPacket`s and `terminate` to gracefully close the session.
pub struct RtpSender<S: Socket + Send + Sync + 'static> {
    rtp_socket: S,
    report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
    connected: Arc<AtomicBool>,
}

impl<S: Socket + Send + Sync + 'static> RtpSender<S> {
    /// Construct a new `RtpSender` using the provided RTP and RTCP
    /// sockets, and the specified `ssrc` identifier.
    ///
    /// The RTCP report handler is started; on failure an `RtpError` is
    /// returned.
    pub fn new(
        rtp_socket: S,
        report_handler: &Arc<Mutex<RtcpReportHandler<S>>>,
        connected: &Arc<AtomicBool>,
    ) -> Result<Self, Error> {
        Ok(Self {
            rtp_socket,
            report_handler: Arc::clone(report_handler),
            connected: Arc::clone(connected),
        })
    }

    pub fn start(&self, event_tx: Sender<AppEvent>) -> Result<Sender<Vec<u8>>, Error> {
        let (tx, rx) = mpsc::channel();

        let rtp_socket = self
            .rtp_socket
            .try_clone()
            .map_err(|_| Error::SocketCloneFailed)?;
        let report_handler = Arc::clone(&self.report_handler);
        let connected = Arc::clone(&self.connected);

        thread::spawn(move || {
            loop {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                let protected_data = match rx.recv() {
                    Ok(p) => {
                        p
                    },
                    Err(e) => {
                        break;
                    }
                };


                if let Err(_) = send_packet(&rtp_socket, &report_handler, &connected, protected_data) {
                    break;
                }
            }
            
            if connected.load(Ordering::SeqCst) {
                connected.store(false, Ordering::SeqCst);
                event_tx.send(AppEvent::CallEnded);
            }
        });

        Ok(tx)
    }
}

/// Send an RTP packet created from the provided payload and metadata.
///
/// The method checks the connection status first. If the underlying
/// socket send fails it attempts to close the RTCP handler and
/// returns an appropriate `RtpError`.
fn send_packet<S: Socket + Send + Sync + 'static>(
    socket: &S,
    report_handler: &Arc<Mutex<RtcpReportHandler<S>>>,
    connected: &AtomicBool,
    protected_packet: Vec<u8>,
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
    Ok(())
}
