use crate::rtcp::RtcpReportHandler;
use crate::rtp::ConnectionStatus;
use crate::rtp::error::RtpError as Error;
use crate::rtp::rtp_packet::RtpPacket;
use crate::srtp::SrtpContext;
use crate::tools::Socket;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// RTP receiver that reads `RtpPacket` instances from a socket and
/// manages RTCP reporting through a `RtcpReportHandler`.
///
/// This type owns a socket implementing the project's `Socket` trait,
/// a locked `RtcpReportHandler` used to drive RTCP-style reporting and
/// a shared `connection_status` used to observe/drive session state.
pub struct RtpReceiver<S: Socket + Send + Sync + 'static> {
    rtp_socket: UdpSocket,
    report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
    connection_status: Arc<RwLock<ConnectionStatus>>,
    max_udp_packet_size: usize,
    srtp_context: Arc<Mutex<SrtpContext>>,
    is_client: bool,
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
        rtp_socket: UdpSocket,
        report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
        connection_status: Arc<RwLock<ConnectionStatus>>,
        rtp_read_timeout_millis: u64,
        max_udp_packet_size: usize,
        srtp_context: Arc<Mutex<SrtpContext>>,
        is_client: bool,
    ) -> Result<Self, Error> {
        rtp_socket
            .set_read_timeout(Some(Duration::from_millis(rtp_read_timeout_millis)))
            .map_err(|_| Error::SocketConfigFailed)?;

        Ok(Self {
            rtp_socket,
            report_handler,
            connection_status,
            max_udp_packet_size,
            srtp_context,
            is_client,
        })
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
    pub fn receive(&mut self) -> Result<RtpPacket, Error> {
        let mut buf = vec![0u8; self.max_udp_packet_size];
        loop {
            match self.rtp_socket.recv_from(&mut buf) {
                Ok((size, _addr)) => {
                    let packet_data = &buf[..size];
                    if packet_data.is_empty() {
                        continue;
                    }

                    let first_byte = packet_data[0];

                    // Filter out DTLS packets (20-63)
                    if (20..=63).contains(&first_byte) {
                        continue;
                    }
                    // Process SRTP/RTP packets (128-191)
                    else if (128..=191).contains(&first_byte) {
                        let unprotected_packet = self
                            .srtp_context
                            .lock()
                            .map_err(|_| Error::PoisonedLock)?
                            .unprotect(packet_data, self.is_client)
                            .map_err(|e| Error::ReceiveFailed(e.to_string()))?;
                        return Ok(unprotected_packet);
                    } else {
                        continue;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if self
                        .connection_status
                        .read()
                        .map_err(|_| Error::ConnectionStatusLockFailed)
                        .map(|conn| *conn == ConnectionStatus::Closed)?
                    {
                        return Err(Error::ConnectionClosed);
                    }
                }
                Err(e) => {
                    self.report_handler
                        .lock()
                        .map_err(|_| Error::PoisonedLock)?
                        .close_connection()
                        .map_err(|e| Error::RTCPError(e.to_string()))?;
                    return Err(Error::ReceiveFailed(e.to_string()));
                }
            }
        }
    }
}
