use chrono::{DateTime, Local};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use crate::rtcp::RtcpError;
use crate::rtcp::RtcpPacket;
use crate::rtp::ConnectionStatus;
use crate::tools::Socket;

const REPORT_PERIOD_MILLIS: u64 = 1000;
const REPORT_RECEIVE_LIMIT: Duration = Duration::from_millis(3000);
const RETRY_LIMIT: usize = 5;

pub struct RtcpReportHandler<S: Socket + Send + Sync + 'static> {
    socket: Arc<S>,
    connection_status: Arc<RwLock<ConnectionStatus>>,
}

impl<S: Socket + Send + Sync + 'static> RtcpReportHandler<S> {
    pub fn new(socket: S, connection_status: Arc<RwLock<ConnectionStatus>>) -> Self {
        Self {
            socket: Arc::new(socket),
            connection_status,
        }
    }

    pub fn start(&self) -> Result<(), RtcpError> {
        let sender_socket = Arc::clone(&self.socket);
        let receiver_socket = Arc::clone(&self.socket);

        self.start_report_receiver(receiver_socket)?;
        self.start_report_sender(sender_socket);
        Ok(())
    }

    fn start_report_sender(&self, report_socket: Arc<S>) {
        let shared_connection_status = Arc::clone(&self.connection_status);
        thread::spawn(move || {
            loop {
                if let Ok(conn) = shared_connection_status.read()
                    && *conn == ConnectionStatus::Closed {
                    break;
                }

                if report_socket
                    .send(RtcpPacket::ConnectivityReport.as_bytes())
                    .is_err()
                {
                    if let Ok(mut conn) = shared_connection_status.write() {
                        *conn = ConnectionStatus::Closed;
                    }
                    break;
                }

                thread::sleep(Duration::from_millis(REPORT_PERIOD_MILLIS));
            }
        });
    }

    fn start_report_receiver(&self, report_socket: Arc<S>) -> Result<(), RtcpError> {
        report_socket
            .set_read_timeout(Some(REPORT_RECEIVE_LIMIT))
            .map_err(|_| RtcpError::SocketConfigFailed)?;

        let shared_connection_status = Arc::clone(&self.connection_status);

        thread::spawn(move || {
            let mut last_report_time = Local::now();
            let mut retries = 0;

            while retries < RETRY_LIMIT {
                if let Ok(conn) = shared_connection_status.read()
                    && *conn == ConnectionStatus::Closed
                {
                    break;
                }

                match try_receive_report(&*report_socket, &mut last_report_time) {
                    Ok(_) => retries = 0,
                    Err(RtcpError::GoodbyeReceived) => retries = RETRY_LIMIT,
                    Err(_) => retries += 1,
                };
            }

            if let Ok(mut conn) = shared_connection_status.write() {
                *conn = ConnectionStatus::Closed;
            }
        });

        Ok(())
    }

    pub fn close_connection(&self) -> Result<(), RtcpError> {
        if let Ok(mut conn) = self.connection_status.write() {
            *conn = ConnectionStatus::Closed;
        }

        let _ = self.socket.send(RtcpPacket::Goodbye.as_bytes());
        Ok(())
    }
}

fn try_receive_report<S: Socket + Send + Sync + 'static>(
    report_socket: &S,
    last_report_time: &mut DateTime<Local>,
) -> Result<(), RtcpError> {
    let mut buf = [0u8; 1024];

    match report_socket.recv_from(&mut buf) {
        Ok((size, _src_addr)) => match RtcpPacket::from_bytes(&buf[..size]) {
            Some(RtcpPacket::ConnectivityReport) => {
                *last_report_time = Local::now();
                Ok(())
            }
            Some(RtcpPacket::Goodbye) => Err(RtcpError::GoodbyeReceived),
            None => {
                if Local::now() - *last_report_time
                    > chrono::Duration::from_std(REPORT_RECEIVE_LIMIT).unwrap()
                {
                    Err(RtcpError::TimedOut)
                } else {
                    Ok(())
                }
            },
        },
        Err(_) => {
            if Local::now() - *last_report_time
                > chrono::Duration::from_std(REPORT_RECEIVE_LIMIT)
                    .map_err(|_| RtcpError::InvalidConfigDuration)?
            {
                Err(RtcpError::TimedOut)
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtcp::RtcpPacket;
    use crate::rtp::ConnectionStatus;
    use crate::tools::MockSocket;
    use std::sync::{Arc, Mutex, RwLock};

    #[test]
    fn test_report_handler_receives_connectivity_report() -> Result<(), RtcpError> {
        let data_to_receive = vec![RtcpPacket::ConnectivityReport.as_bytes().to_vec()];
        let sent_data = Arc::new(Mutex::new(Vec::new()));
        let mock_socket = MockSocket {
            data_to_receive,
            sent_data: sent_data.clone(),
        };

        let connection_status = Arc::new(RwLock::new(ConnectionStatus::Open));
        let handler = RtcpReportHandler::new(mock_socket, Arc::clone(&connection_status));

        handler.start()?;
        thread::sleep(Duration::from_millis(100));

        let status = connection_status
            .read()
            .map_err(|_| RtcpError::ConnectionStatusLockFailed)?;
        assert_eq!(*status, ConnectionStatus::Open);

        let sent = sent_data.lock().map_err(|_| RtcpError::PoisonedLock)?;
        assert_eq!(sent[0], RtcpPacket::ConnectivityReport.as_bytes());
        Ok(())
    }

    #[test]
    fn test_close_connection_sets_status_closed() -> Result<(), RtcpError> {
        let mock_socket = MockSocket {
            data_to_receive: vec![],
            sent_data: Arc::new(Mutex::new(Vec::new())),
        };
        let connection_status = Arc::new(RwLock::new(ConnectionStatus::Open));
        let handler = RtcpReportHandler::new(mock_socket, Arc::clone(&connection_status));

        handler.close_connection()?;

        let status = connection_status
            .read()
            .map_err(|_| RtcpError::ConnectionStatusLockFailed)?;
        assert_eq!(*status, ConnectionStatus::Closed);
        Ok(())
    }

    #[test]
    fn test_connection_closes_after_inactivity() -> Result<(), RtcpError> {
        let mock_socket = MockSocket {
            data_to_receive: vec![],
            sent_data: Arc::new(Mutex::new(Vec::new())),
        };

        let connection_status = Arc::new(RwLock::new(ConnectionStatus::Open));
        let handler = RtcpReportHandler::new(mock_socket, Arc::clone(&connection_status));

        handler.start()?;

        let wait_time = REPORT_RECEIVE_LIMIT * (RETRY_LIMIT as u32 + 1);
        thread::sleep(wait_time);

        let status = connection_status
            .read()
            .map_err(|_| RtcpError::ConnectionStatusLockFailed)?;
        assert_eq!(*status, ConnectionStatus::Closed);
        Ok(())
    }
}
