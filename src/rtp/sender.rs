use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, RwLock};
use crate::rtcp::rtcp_connection_handler::RTCPReportHandler;
use crate::rtp::connection_status::ConnectionStatus;
use crate::rtp::error::RTPError;
use crate::rtp::rtp_package::RtpPackage;

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
        rtp_socket.connect(rtp_dest).map_err(|_| RTPError::AddrNotAvailable)?;

        let rtcp_socket = UdpSocket::bind("0.0.0.0:0").map_err(|_| RTPError::AddrNotAvailable)?;
        rtcp_socket.connect(rtcp_dest).map_err(|_| RTPError::AddrNotAvailable)?;

        let connection_status = Arc::new(RwLock::new(ConnectionStatus::Open));

        let report_handler = RTCPReportHandler::new(rtcp_socket, Arc::clone(&connection_status));
        report_handler.start().map_err(|e| RTPError::RTCPError(e.to_string()))?;

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
        marker: bool,
    ) -> Result<(), RTPError> {
        let rtp_package = RtpPackage::new(
            marker,
            payload_type,
            payload.to_vec(),
            timestamp,
            self.sequence_number,
            self.ssrc,
        );

        let data = rtp_package.to_bytes();
        self.rtp_socket.send(&data).map_err(|_| RTPError::SendFailed)?;

        self.sequence_number = self.sequence_number.wrapping_add(1);

        Ok(())
    }

    pub fn terminate(&mut self) -> Result<(), RTPError> {
        self.report_handler.close_connection().map_err(|_| RTPError::TerminateFailed)
    }
}