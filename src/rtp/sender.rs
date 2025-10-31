use crate::rtcp::RtcpReportHandler;
use crate::rtp::connection_status::ConnectionStatus;
use crate::rtp::error::RtpError;
use crate::rtp::rtp_packet::RtpPacket;
use crate::tools::Socket;
use std::sync::{Arc, RwLock};

pub struct RtpSender<S: Socket + Send + Sync + 'static> {
    rtp_socket: S,
    report_handler: RtcpReportHandler<S>,
    connection_status: Arc<RwLock<ConnectionStatus>>,
    ssrc: u32,
}

impl<S: Socket + Send + Sync + 'static> RtpSender<S> {
    pub fn new(rtp_socket: S, rtcp_socket: S, ssrc: u32) -> Result<Self, RtpError> {
        let connection_status = Arc::new(RwLock::new(ConnectionStatus::Open));

        let report_handler = RtcpReportHandler::new(rtcp_socket, Arc::clone(&connection_status));
        report_handler
            .start()
            .map_err(|e| RtpError::RTCPError(e.to_string()))?;

        Ok(Self {
            rtp_socket,
            report_handler,
            connection_status,
            ssrc,
        })
    }

    pub fn send(
        &mut self,
        payload: &[u8],
        payload_type: u8,
        timestamp: u32,
        frame_id: u64,
        chunk_id: u64,
        marker: u16,
    ) -> Result<(), RtpError> {
        if let Ok(conn) = self.connection_status.read()
            && *conn == ConnectionStatus::Closed
        {
            return Err(RtpError::ConnectionClosed);
        }

        let rtp_package = RtpPacket::new(
            marker,
            payload_type,
            payload.to_vec(),
            timestamp,
            frame_id,
            chunk_id,
            self.ssrc,
        );

        let data = rtp_package.to_bytes();

        if self.rtp_socket.send(&data).is_err() {
            self.report_handler
                .close_connection()
                .map_err(|e| RtpError::RTCPError(e.to_string()))?;
            return Err(RtpError::SendFailed);
        }

        Ok(())
    }

    pub fn terminate(&mut self) -> Result<(), RtpError> {
        self.report_handler
            .close_connection()
            .map_err(|_| RtpError::TerminateFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtcp::RtcpPacket;
    use crate::rtp::ConnectionStatus;
    use crate::tools::MockSocket;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_send_multiple_rtp_packets_increments_sequence() -> Result<(), RtpError> {
        let rtp_sent = Arc::new(Mutex::new(Vec::new()));
        let rtcp_sent = Arc::new(Mutex::new(Vec::new()));

        let rtp_socket = MockSocket {
            data_to_receive: vec![],
            sent_data: Arc::clone(&rtp_sent),
        };

        let rtcp_socket = MockSocket {
            data_to_receive: vec![],
            sent_data: Arc::clone(&rtcp_sent),
        };

        let mut sender = RtpSender::new(rtp_socket, rtcp_socket, 0)?;

        for i in 0..3 {
            let payload = vec![i];
            sender.send(&payload, 96, 1234 + i as u32, 0, i.into(), 5)?;
        }

        let sent = rtp_sent.lock().map_err(|_| RtpError::PoisonedLock)?;
        assert_eq!(sent.len(), 3, "There should have been three packets sent");
        Ok(())
    }

    #[test]
    fn test_terminate_closes_connection_and_sends_goodbye() -> Result<(), RtpError> {
        let rtp_sent = Arc::new(Mutex::new(Vec::new()));
        let rtcp_sent = Arc::new(Mutex::new(Vec::new()));

        let rtp_socket = MockSocket {
            data_to_receive: vec![],
            sent_data: Arc::clone(&rtp_sent),
        };

        let rtcp_socket = MockSocket {
            data_to_receive: vec![],
            sent_data: Arc::clone(&rtcp_sent),
        };

        let mut sender = RtpSender::new(rtp_socket, rtcp_socket, 0)?;

        sender.terminate()?;

        let status = sender.connection_status.read().map_err(|_| RtpError::ConnectionStatusLockFailed)?;
        assert_eq!(*status, ConnectionStatus::Closed);

        let rtcp_sent_data = rtcp_sent.lock().map_err(|_| RtpError::PoisonedLock)?;
        assert!(
            rtcp_sent_data
                .iter()
                .any(|d| d == &RtcpPacket::Goodbye.as_bytes().to_vec()),
            "A Goodbye packet should have been sent"
        );
        Ok(())
    }
}
