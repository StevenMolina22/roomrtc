use crate::rtcp::RtcpReportHandler;
use crate::rtp::ConnectionStatus;
use crate::rtp::error::RtpError as Error;
use crate::rtp::rtp_packet::RtpPacket;
use crate::tools::Socket;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

/// Number of milliseconds used as the read timeout on the RTP socket.
const RTP_READ_TIMEOUT_MILLIS: u64 = 3000;

/// RTP receiver that reads `RtpPacket` instances from a socket and
/// manages RTCP reporting through a `RtcpReportHandler`.
///
/// This type owns a socket implementing the project's `Socket` trait,
/// a locked `RtcpReportHandler` used to drive RTCP-style reporting and
/// a shared `connection_status` used to observe/drive session state.
pub struct RtpReceiver<S: Socket + Send + Sync + 'static> {
    rtp_socket: S,
    report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
    connection_status: Arc<RwLock<ConnectionStatus>>,
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
        rtp_socket: S,
        report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
        connection_status: Arc<RwLock<ConnectionStatus>>,
    ) -> Result<Self, Error> {
        rtp_socket
            .set_read_timeout(Some(Duration::from_millis(RTP_READ_TIMEOUT_MILLIS)))
            .map_err(|_| Error::SocketConfigFailed)?;

        Ok(Self {
            rtp_socket,
            report_handler,
            connection_status,
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
        let mut buf = vec![0u8; 65535];
        loop {
            match self.rtp_socket.recv_from(&mut buf) {
                Ok((size, _addr)) => {
                    if let Some(packet) = RtpPacket::from_bytes(&buf[..size]) {
                        return Ok(packet);
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

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::tools::MockSocket;
//     use std::sync::{Arc, Mutex};
//
//     #[test]
//     fn test_receiver_receives_rtp_packet() -> Result<(), Error> {
//         let fake_payload = vec![1, 2, 3, 4];
//         let fake_rtp_packet = RtpPacket::new(5, 96, fake_payload.clone(), 1234, 0, 0, 42);
//         let rtp_data = vec![fake_rtp_packet.to_bytes()];
//         let rtp_sent = Arc::new(Mutex::new(Vec::new()));
//
//         let rtp_socket = MockSocket {
//             data_to_receive: rtp_data,
//             sent_data: Arc::clone(&rtp_sent),
//         };
//
//         let rtcp_socket = MockSocket {
//             data_to_receive: vec![],
//             sent_data: Arc::new(Mutex::new(Vec::new())),
//         };
//
//         let mut receiver = RtpReceiver::new(rtp_socket, rtcp_socket, Arc::new(RwLock::new(ConnectionStatus::Open)))?;
//         let received = receiver.receive()?;
//
//         assert_eq!(received.payload, fake_payload);
//         assert_eq!(received.payload_type, 96);
//         assert_eq!(received.timestamp, 1234);
//         assert_eq!(received.frame_id, 0);
//         assert_eq!(received.chunk_id, 0);
//         assert_eq!(received.ssrc, 42);
//
//         Ok(())
//     }
// }
