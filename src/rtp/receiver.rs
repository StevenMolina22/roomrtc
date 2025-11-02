use crate::rtcp::RtcpReportHandler;
use crate::rtp::ConnectionStatus;
use crate::rtp::error::RtpError;
use crate::rtp::rtp_packet::RtpPacket;
use crate::tools::Socket;
use std::sync::{Arc, RwLock};
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
    report_handler: RtcpReportHandler<S>,
    connection_status: Arc<RwLock<ConnectionStatus>>,
}

impl<S: Socket + Send + Sync + 'static> RtpReceiver<S> {
    /// Create a new `RtpReceiver` using the provided RTP and RTCP
    /// sockets.
    ///
    /// This configures a read timeout on the RTP socket and starts the
    /// RTCP report handler in the background. Returns an `RtpError` if
    /// configuration or RTCP handler start fails.
    pub fn new(rtp_socket: S, rtcp_socket: S, connection_status: Arc<RwLock<ConnectionStatus>>) -> Result<Self, RtpError> {
        rtp_socket
            .set_read_timeout(Some(Duration::from_millis(RTP_READ_TIMEOUT_MILLIS)))
            .map_err(|_| RtpError::SocketConfigFailed)?;

        let report_handler = RtcpReportHandler::new(rtcp_socket, Arc::clone(&connection_status));
        report_handler
            .start()
            .map_err(|e| RtpError::RTCPError(e.to_string()))?;

        Ok(Self {
            rtp_socket,
            report_handler,
            connection_status,
        })
    }

    /// Attempt to receive and decode a single `RtpPacket`.
    ///
    /// The function blocks until a packet is received or an error
    /// condition occurs. If the underlying socket times out, the method
    /// checks the connection status; if the connection was closed, it
    /// returns `RtpError::ConnectionClosed`, otherwise it keeps waiting.
    ///
    /// On success returns the decoded `RtpPacket`.
    pub fn receive(&mut self) -> Result<RtpPacket, RtpError> {
        let mut buf = [0u8; 65535];
        loop {
            match self.rtp_socket.recv_from(&mut buf) {
                Ok((size, _addr)) => {
                    if let Some(packet) = RtpPacket::from_bytes(&buf[..size]) {
                        return Ok(packet);
                    } else {
                        continue;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let conn = self.connection_status.read().map_err(|_| RtpError::ConnectionStatusLockFailed)?;
                    if *conn == ConnectionStatus::Closed {
                        return Err(RtpError::ConnectionClosed);
                    } else {
                        continue;
                    }
                }
                Err(_) => {
                    self.report_handler
                        .close_connection()
                        .map_err(|e| RtpError::RTCPError(e.to_string()))?;
                    return Err(RtpError::ReceiveFailed);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::MockSocket;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_receiver_receives_rtp_packet() -> Result<(), RtpError> {
        let fake_payload = vec![1, 2, 3, 4];
        let fake_rtp_packet = RtpPacket::new(5, 96, fake_payload.clone(), 1234, 0, 0, 42);
        let rtp_data = vec![fake_rtp_packet.to_bytes()];
        let rtp_sent = Arc::new(Mutex::new(Vec::new()));

        let rtp_socket = MockSocket {
            data_to_receive: rtp_data,
            sent_data: Arc::clone(&rtp_sent),
        };

        let rtcp_socket = MockSocket {
            data_to_receive: vec![],
            sent_data: Arc::new(Mutex::new(Vec::new())),
        };

        let mut receiver = RtpReceiver::new(rtp_socket, rtcp_socket, Arc::new(RwLock::new(ConnectionStatus::Open)))?;
        let received = receiver.receive()?;

        assert_eq!(received.payload, fake_payload);
        assert_eq!(received.payload_type, 96);
        assert_eq!(received.timestamp, 1234);
        assert_eq!(received.frame_id, 0);
        assert_eq!(received.chunk_id, 0);
        assert_eq!(received.ssrc, 42);

        Ok(())
    }
}
