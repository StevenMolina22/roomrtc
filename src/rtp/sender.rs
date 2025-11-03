use std::sync::{Arc, Mutex, RwLock};

use crate::rtcp::RtcpReportHandler;
use crate::rtp::ConnectionStatus;
use crate::rtp::error::RtpError as Error;
use crate::rtp::rtp_packet::RtpPacket;
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
    connection_status: Arc<RwLock<ConnectionStatus>>,
    rtp_version: u8,
}

impl<S: Socket + Send + Sync + 'static> RtpSender<S> {
    /// Construct a new `RtpSender` using the provided RTP and RTCP
    /// sockets, and the specified `ssrc` identifier.
    ///
    /// The RTCP report handler is started; on failure an `RtpError` is
    /// returned.
    pub const fn new(
        rtp_socket: S,
        report_handler: Arc<Mutex<RtcpReportHandler<S>>>,
        ssrc: u32,
        connection_status: Arc<RwLock<ConnectionStatus>>,
        rtp_version: u8,
    ) -> Result<Self, Error> {
        Ok(Self {
            rtp_socket,
            report_handler,
            ssrc,
            connection_status,
            rtp_version,
        })
    }

    /// Send an RTP packet created from the provided payload and metadata.
    ///
    /// The method checks the connection status first. If the underlying
    /// socket send fails it attempts to close the RTCP handler and
    /// returns an appropriate `RtpError`.
    pub fn send(
        &mut self,
        payload: &[u8],
        payload_type: u8,
        timestamp: u32,
        frame_id: u64,
        chunk_id: u64,
        marker: u16,
    ) -> Result<(), Error> {
        if let Ok(conn) = self.connection_status.read()
            && *conn == ConnectionStatus::Closed
        {
            return Err(Error::ConnectionClosed);
        }

        let rtp_package = RtpPacket::new(
            self.rtp_version,
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
                .lock()
                .map_err(|_| Error::PoisonedLock)?
                .close_connection()
                .map_err(|e| Error::RTCPError(e.to_string()))?;
            return Err(Error::SendFailed);
        }

        Ok(())
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::rtp::ConnectionStatus;
//     use crate::tools::MockSocket;
//     use std::sync::{Arc, Mutex};
//
//     #[test]
//     fn test_send_multiple_rtp_packets_increments_sequence() -> Result<(), Error> {
//         let rtp_sent = Arc::new(Mutex::new(Vec::new()));
//         let rtcp_sent = Arc::new(Mutex::new(Vec::new()));
//
//         let rtp_socket = MockSocket {
//             data_to_receive: vec![],
//             sent_data: Arc::clone(&rtp_sent),
//         };
//
//         let rtcp_socket = MockSocket {
//             data_to_receive: vec![],
//             sent_data: Arc::clone(&rtcp_sent),
//         };
//
//         let mut sender = RtpSender::new(
//             rtp_socket,
//             rtcp_socket,
//             0,
//             Arc::new(RwLock::new(ConnectionStatus::Open)),
//         )?;
//
//         for i in 0..3 {
//             let payload = vec![i];
//             sender.send(&payload, 96, 1234 + i as u32, 0, i.into(), 5)?;
//         }
//
//         let sent = rtp_sent.lock().unwrap();
//         assert_eq!(sent.len(), 3, "There should have been three packets sent");
//         Ok(())
//     }
// }
