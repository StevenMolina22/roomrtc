use crate::rtcp::RTCPError;
use crate::rtcp::RTCPPackage;
use crate::rtp::connection_status::ConnectionStatus;
use chrono::{DateTime, Local};
use std::net::UdpSocket;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

const REPORT_PERIOD_SEC: u64 = 3;
const REPORT_RECEIVE_LIMIT: Duration = Duration::from_secs(5);
const RETRY_LIMIT: usize = 3;

pub struct RTCPReportHandler {
    socket: UdpSocket,
    connection_status: Arc<RwLock<ConnectionStatus>>,
}

impl RTCPReportHandler {
    pub fn new(socket: UdpSocket, connection_status: Arc<RwLock<ConnectionStatus>>) -> Self {
        Self {
            socket,
            connection_status,
        }
    }

    pub fn start(&self) -> Result<(), RTCPError> {
        let sender_socket = self
            .socket
            .try_clone()
            .map_err(|_| RTCPError::SocketCloneFailed)?;
        let receiver_socket = self
            .socket
            .try_clone()
            .map_err(|_| RTCPError::SocketCloneFailed)?;
        self.start_report_receiver(sender_socket)?;
        self.start_report_sender(receiver_socket);
        Ok(())
    }

    fn start_report_sender(&self, report_socket: UdpSocket) {
        let shared_connection_status: Arc<RwLock<ConnectionStatus>> =
            Arc::clone(&self.connection_status);
        thread::spawn(move || {
            loop {
                if let Ok(conn) = shared_connection_status.read() {
                    if *conn == ConnectionStatus::Closed {
                        break;
                    }
                }

                if report_socket
                    .send(RTCPPackage::ConnectivityReport.as_bytes())
                    .is_err()
                {
                    if let Ok(mut conn) = shared_connection_status.write() {
                        *conn = ConnectionStatus::Closed;
                    }
                    break;
                } else {
                    thread::sleep(Duration::from_secs(REPORT_PERIOD_SEC));
                }
            }
        });
    }

    fn start_report_receiver(&self, report_socket: UdpSocket) -> Result<(), RTCPError> {
        report_socket
            .set_read_timeout(Some(REPORT_RECEIVE_LIMIT))
            .map_err(|_| RTCPError::SocketConfigFailed)?;
        let shared_connection_status: Arc<RwLock<ConnectionStatus>> =
            Arc::clone(&self.connection_status);

        thread::spawn(move || {
            let mut last_report_time = Local::now();
            let mut retries = 0;

            while retries < RETRY_LIMIT {
                if let Ok(conn) = shared_connection_status.read() {
                    if *conn == ConnectionStatus::Closed {
                        break;
                    }
                }

                match try_receive_report(&report_socket, &mut last_report_time) {
                    Ok(_) => retries = 0,
                    Err(RTCPError::GoodbyeReceived) => retries = RETRY_LIMIT,
                    Err(_) => retries += 1,
                };
            }

            if let Ok(mut conn) = shared_connection_status.write() {
                *conn = ConnectionStatus::Closed;
            }
        });
        Ok(())
    }

    pub fn close_connection(&self) -> Result<(), RTCPError> {
        if let Ok(mut conn) = self.connection_status.write() {
            *conn = ConnectionStatus::Closed;
        }

        let _ = self.socket.send(RTCPPackage::Goodbye.as_bytes());
        Ok(())
    }
}

fn try_receive_report(
    report_socket: &UdpSocket,
    last_report_time: &mut DateTime<Local>,
) -> Result<(), RTCPError> {
    let mut buf = [0u8; 1024];
    match report_socket.recv_from(&mut buf) {
        Ok((size, _src_addr)) => match RTCPPackage::from_bytes(&buf[..size]) {
            Some(RTCPPackage::ConnectivityReport) => {
                *last_report_time = Local::now();
                Ok(())
            }
            Some(RTCPPackage::Goodbye) => Err(RTCPError::GoodbyeReceived),
            None => Err(RTCPError::TimedOut),
        },
        Err(_) => {
            if Local::now() - *last_report_time
                > chrono::Duration::from_std(REPORT_RECEIVE_LIMIT)
                    .unwrap_or_else(|_| chrono::Duration::seconds(30))
            {
                Err(RTCPError::TimedOut)
            } else {
                Ok(())
            }
        }
    }
}
