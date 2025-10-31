use crate::rtcp::rtcp_connection_handler::RTCPReportHandler;
use crate::rtp::connection_status::ConnectionStatus;
use crate::rtp::error::RTPError;
use crate::rtp::rtp_package::RtpPackage;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, RwLock};

pub struct RtpSender {
    rtp_socket: UdpSocket,
    report_handler: RTCPReportHandler,
    sequence_number: u16,
    ssrc: u32,
    connection_status: Arc<RwLock<ConnectionStatus>>,
}

impl RtpSender {
    pub fn new(rtp_dest: SocketAddr, rtcp_dest: SocketAddr, ssrc: u32) -> Result<Self, RTPError> {
        let rtp_socket = UdpSocket::bind("0.0.0.0:0").map_err(|_| RTPError::AddrNotAvailable)?;
        rtp_socket
            .connect(rtp_dest)
            .map_err(|_| RTPError::AddrNotAvailable)?;

        let rtcp_socket = UdpSocket::bind("0.0.0.0:0").map_err(|_| RTPError::AddrNotAvailable)?;
        rtcp_socket
            .connect(rtcp_dest)
            .map_err(|_| RTPError::AddrNotAvailable)?;

        let connection_status = Arc::new(RwLock::new(ConnectionStatus::Open));

        let report_handler = RTCPReportHandler::new(rtcp_socket, Arc::clone(&connection_status));
        report_handler
            .start()
            .map_err(|e| RTPError::RTCPError(e.to_string()))?;

        Ok(Self {
            rtp_socket,
            report_handler,
            sequence_number: 0,
            ssrc,
            connection_status,
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
            self.sequence_number,
            self.ssrc,
        );

        let data = rtp_package.to_bytes();
        self.rtp_socket
            .send(&data)
            .map_err(|_| RTPError::SendFailed)?;

        self.sequence_number = self.sequence_number.wrapping_add(1);

        Ok(())
    }

    pub fn terminate(&mut self) -> Result<(), RTPError> {
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

        let sent = rtp_sent.lock().unwrap();
        assert_eq!(sent.len(), 3, "There should have been three packets sent");
        Ok(())
    }

    #[test]
    fn test_terminate_closes_connection_and_sends_goodbye() {
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

        let mut sender = RtpSender::new(rtp_socket, rtcp_socket, 0).unwrap();

        sender.terminate().unwrap();

        let status = sender.connection_status.read().unwrap();
        assert_eq!(*status, ConnectionStatus::Closed);

        let rtcp_sent_data = rtcp_sent.lock().unwrap();
        assert!(
            rtcp_sent_data
                .iter()
                .any(|d| d == &RtcpPacket::Goodbye.as_bytes().to_vec()),
            "A Goodbye packet should have been sent"
        );
    }
}
