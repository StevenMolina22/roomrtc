use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender};
use std::thread;
use crate::config::Config;
use crate::media::frame_handler::EncodedFrame;
use crate::transport::rtcp::RtcpReportHandler;
use crate::transport::rtp::error::RtpError as Error;
use crate::transport::rtp::rtp_packet::RtpPacket;
use crate::tools::Socket;
use chrono::Local;

/// RTP sender that transmits `RtpPacket`s and manages RTCP reporting.
///
/// The sender wraps a socket implementing the project's `Socket` trait
/// and an RTCP report handler. It offers `send` for sending payloads as
/// `RtpPacket`s and `terminate` to gracefully close the session.
pub struct RtpSender<S: Socket + Send + Sync + 'static> {
    rtp_socket: S,
    report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
    connected: Arc<AtomicBool>,
    config: Arc<Config>,
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
        config: &Arc<Config>,
    ) -> Result<Self, Error> {
        Ok(Self {
            rtp_socket,
            report_handler: Arc::clone(report_handler),
            connected: Arc::clone(connected),
            config: Arc::clone(config),
        })
    }
    
    pub fn start(&self) -> Result<Sender<EncodedFrame>, Error> {
        let (tx, rx) = mpsc::channel();

        let rtp_socket = self.rtp_socket.try_clone().map_err(|_| Error::SocketCloneFailed)?;
        let report_handler = Arc::clone(&self.report_handler);
        let connected = Arc::clone(&self.connected);
        let config = self.config.clone();

        thread::spawn(move || {
            loop {
                if !connected.load(Ordering::SeqCst) {
                    println!("RTP sender thread disconnected");
                    break;
                }

                let enc_frame = match rx.recv() {
                    Ok(p) => p,
                    Err(e) => {
                        println!("Error rtp sender recv: {e}");
                        break;
                    },
                };

                if let Err(e) = send_packet(&rtp_socket, &report_handler, &connected, enc_frame, &config) {
                    println!("Error sending rtp packet: {e}");
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
    enc_frame: EncodedFrame,
    config: &Arc<Config>,
) -> Result<(), Error> {
    if !connected.load(Ordering::SeqCst) {
        return Err(Error::ConnectionClosed);
    }

    let marker = enc_frame.chunks.len() as u16;
    for (chunk_id, payload) in enc_frame.chunks.iter().enumerate() {
        let packet = RtpPacket {
            version: config.media.rtp_version,
            marker,
            payload_type: config.media.rtp_payload_type,
            frame_id: enc_frame.id,
            chunk_id: chunk_id as u64,
            timestamp: Local::now().timestamp_millis() as u32,
            ssrc: 12345,
            payload: payload.to_vec(),
        };

        if socket.send(&packet.to_bytes()).is_err() {
            report_handler
                .lock()
                .map_err(|_| Error::PoisonedLock)?
                .close_connection()
                .map_err(|e| Error::RTCPError(e.to_string()))?;

            return Err(Error::SendFailed);
        }
    }
    Ok(())
}
