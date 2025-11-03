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
/// The `RtpReceiver` owns a UDP socket implementing the project's
/// `Socket` trait and an RTCP handler. It tracks the connection status
/// and exposes a blocking `receive` method that returns decoded
/// `RtpPacket`s.
pub struct RtpReceiver<S: Socket + Send + Sync + 'static> {
    rtp_socket: S,
    report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
    connection_status: Arc<RwLock<ConnectionStatus>>,
}

impl<S: Socket + Send + Sync + 'static> RtpReceiver<S> {
    /// Creates an RTP receptor bound to the local IP at the given port
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

    /// Attempts to receive an RTP packet. Returns Some(RtpPackage) if a packet was received, or None if no data is available.
    pub fn receive(&mut self) -> Result<RtpPacket, Error> {
        let mut buf = [0u8; 65535];
        loop {
            match self.rtp_socket.recv_from(&mut buf) {
                Ok((size, _addr)) => {
                    if let Some(packet) = RtpPacket::from_bytes(&buf[..size]) {
                        return Ok(packet);
                    }
                    continue;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let conn = self
                        .connection_status
                        .read()
                        .map_err(|_| Error::ConnectionStatusLockFailed)?;
                    if *conn == ConnectionStatus::Closed {
                        return Err(Error::ConnectionClosed);
                    }
                    continue;
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
