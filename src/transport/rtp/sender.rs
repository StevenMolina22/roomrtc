use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtp::error::RtpError as Error;
use crate::transport::rtp::rtp_packet::RtpPacket;
use crate::tools::Socket;

/// RTP sender that transmits `RtpPacket`s and manages RTCP reporting.
///
/// The sender wraps a socket implementing the project's `Socket` trait
/// and an RTCP report handler. It offers `send` for sending payloads as
/// `RtpPacket`s and `terminate` to gracefully close the session.
pub struct RtpSender<S: Socket + Send + Sync + 'static> {
    rtp_socket: S,
    report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
    ssrc: u32,
    connected: Arc<AtomicBool>,
    rtp_version: u8,
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
        ssrc: u32,
        connected: &Arc<AtomicBool>,
        rtp_version: u8,
    ) -> Result<Self, Error> {
        Ok(Self {
            rtp_socket,
            report_handler: Arc::clone(report_handler),
            ssrc,
            connected: Arc::clone(connected),
            rtp_version,
        })
    }
    
    pub fn start(&self) -> Result<Sender<RtpPacket>, Error> {
        let (tx, rx) = mpsc::channel();

        let rtp_socket = self.rtp_socket.try_clone().map_err(|e| Error::SocketCloneFailed)?;
        let report_handler = Arc::clone(&self.report_handler);
        let connected = Arc::clone(&self.connected);
        let ssrc = self.ssrc;
        let version = self.rtp_version;

        thread::spawn(move || {
            loop {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                let packet = match rx.recv() {
                    Ok(p) => p,
                    Err(_) => break,
                };

                if send_packet(&rtp_socket, &report_handler, &connected, packet).is_err() {
                    break;
                }
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
    packet: RtpPacket,
) -> Result<(), Error> {
    if !connected.load(Ordering::SeqCst) {
        return Err(Error::ConnectionClosed);
    }

    if socket.send(&packet.to_bytes()).is_err() {
        report_handler
            .lock()
            .map_err(|_| Error::PoisonedLock)?
            .close_connection()
            .map_err(|e| Error::RTCPError(e.to_string()))?;

        return Err(Error::SendFailed);
    }
    Ok(())
}
