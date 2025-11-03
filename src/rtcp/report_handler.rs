use chrono::{DateTime, Local};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use crate::rtcp::RtcpError as Error;
use crate::rtcp::RtcpPacket;
use crate::rtp::ConnectionStatus;
use crate::tools::Socket;

/// How often (in seconds) a connectivity report is sent.
const REPORT_PERIOD_MILLIS: u64 = 1000;
const REPORT_RECEIVE_LIMIT: Duration = Duration::from_millis(1000);
const RETRY_LIMIT: usize = 5;

/// RTCP report handler that periodically sends connectivity reports and
/// listens for incoming reports (or goodbye messages) from the peer.
///
/// It spawns a sender thread and a receiver thread; both threads share
/// a connection status flag to indicate when the session is closed.
pub struct RtcpReportHandler<S: Socket + Send + Sync + 'static> {
    socket: Arc<S>,
    connection_status: Arc<RwLock<ConnectionStatus>>,
}

impl<S: Socket + Send + Sync + 'static> RtcpReportHandler<S> {
    /// Create a new `RtcpReportHandler` bound to a socket and a shared
    /// connection status handle.
    ///
    /// The handler spawns background threads to periodically send
    /// connectivity reports and to listen for incoming reports or
    /// control messages (e.g. goodbye). The provided `connection_status`
    /// is used to coordinate session state between threads.
    ///
    /// # Parameters
    /// - `socket`: an implementation of the project's `Socket` trait
    ///   used for sending/receiving RTCP-style messages.
    /// - `connection_status`: shared `Arc<RwLock<ConnectionStatus>>` used
    ///   to publish the session state (Open/Closed) across threads.
    ///
    /// # Returns
    /// A configured `RtcpReportHandler` that is ready to `start()`.
    pub fn new(socket: S, connection_status: Arc<RwLock<ConnectionStatus>>) -> Self {
        Self {
            socket: Arc::new(socket),
            connection_status,
        }
    }

    /// Perform the RTCP-style handshake to establish the reporting
    /// session with a remote peer.
    ///
    /// This method sends a `Hello` packet, waits for the peer's
    /// `Hello`, replies with `Ready` and transitions the local
    /// `connection_status` to `Open` once the handshake completes.
    ///
    /// # Errors
    /// Returns an `Error` when socket operations fail or when an
    /// unexpected message is received during the handshake.
    pub fn init_connection(&self) -> Result<(), Error> {
        if let Err(e) = self.socket.send(RtcpPacket::Hello.as_bytes()) {
            eprintln!("{}", e);
        }

        let mut ready = false;

        loop {
            let mut buff = [0u8; 1024];
            match self.socket.recv_from(&mut buff) {
                Ok((size, _addr)) => {
                    match RtcpPacket::from_bytes(&buff[..size]) {
                        Some(RtcpPacket::Hello) => {
                            self.socket.send(RtcpPacket::Ready.as_bytes()).map_err(|e| Error::SendFailed(e.to_string()))?;
                        },
                        Some(RtcpPacket::Ready) => {
                            if ready {
                                break;
                            } else {
                                ready = true;
                                self.socket.send(RtcpPacket::Ready.as_bytes()).map_err(|e| Error::SendFailed(e.to_string()))?;
                                let mut conn = self.connection_status.write()
                                    .map_err(|_| Error::ConnectionStatusLockFailed)?;
                                *conn = ConnectionStatus::Open;
                            }
                        },
                        Some(_) => return Err(Error::UnexpectedMessage),
                        None => continue,
                    }
                }
                Err(e) => return Err(Error::ReceiveFailed(e.to_string())),
            }
        }
        Ok(())
    }
    
    pub fn start(&self) -> Result<(), Error> {
        let sender_socket = Arc::clone(&self.socket);
        let receiver_socket = Arc::clone(&self.socket);
        // Start the receiver thread first so it can immediately observe
        // incoming messages; then start the sender thread which will
        // periodically transmit connectivity reports.
        self.start_report_receiver(receiver_socket)?;
        self.start_report_sender(sender_socket);
        Ok(())
    }

    /// Spawn the background sender thread that periodically sends
    /// connectivity reports until the connection is closed.
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

    /// Spawn the background receiver thread which listens for incoming
    /// RTCP-style packets and updates the `connection_status` accordingly.
    ///
    /// The receiver runs with a read timeout configured to `REPORT_RECEIVE_LIMIT`
    /// and uses `try_receive_report` to parse and handle incoming data.
    fn start_report_receiver(&self, report_socket: Arc<S>) -> Result<(), Error> {
        report_socket
            .set_read_timeout(Some(REPORT_RECEIVE_LIMIT))
            .map_err(|_| Error::SocketConfigFailed)?;

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
                    Err(Error::GoodbyeReceived) => {
                        println!("Goodbye recibido!");
                        retries = RETRY_LIMIT
                    },
                    Err(_) => retries += 1,
                };
            }

            if let Ok(mut conn) = shared_connection_status.write() {
                *conn = ConnectionStatus::Closed;
            }
        });

        Ok(())
    }

    /// Close the reporting connection by sending a `Goodbye` packet
    /// and updating the `connection_status` to `Closed`.
    ///
    /// # Returns
    /// A result indicating success or failure.
    pub fn close_connection(&self) -> Result<(), Error> {
        if let Ok(mut conn) = self.connection_status.write() {
            *conn = ConnectionStatus::Closed;
        }

        println!("envio goodbye");
        let _ = self.socket.send(RtcpPacket::Goodbye.as_bytes());
        Ok(())
    }
}

/// Try to receive a report from the socket. Updates `last_report_time`
/// on successful connectivity reports, returns `GoodbyeReceived` on a
/// goodbye, `TimedOut` when no valid packet is received, and propagates
/// other errors.
fn try_receive_report<S: Socket + Send + Sync + 'static>(
    report_socket: &S,
    last_report_time: &mut DateTime<Local>,
) -> Result<(), Error> {
    let mut buf = [0u8; 1024];

    match report_socket.recv_from(&mut buf) {
        Ok((size, _src_addr)) => match RtcpPacket::from_bytes(&buf[..size]) {
            Some(RtcpPacket::ConnectivityReport) => {
                *last_report_time = Local::now();
                Ok(())
            }
            Some(RtcpPacket::Goodbye) => Err(Error::GoodbyeReceived),
            Some(_) => Err(Error::UnexpectedMessage),
            None => {
                if Local::now() - *last_report_time
                    > chrono::Duration::from_std(REPORT_RECEIVE_LIMIT).unwrap()
                {
                    Err(Error::TimedOut)
                } else {
                    Ok(())
                }
            },
        },
        Err(_) => {
            if Local::now() - *last_report_time
                > chrono::Duration::from_std(REPORT_RECEIVE_LIMIT)
                    .map_err(|_| Error::InvalidConfigDuration)?
            {
                Err(Error::TimedOut)
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
    fn test_report_handler_receives_connectivity_report() -> Result<(), Error> {
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
            .map_err(|_| Error::ConnectionStatusLockFailed)?;
        assert_eq!(*status, ConnectionStatus::Open);

        let sent = sent_data.lock().map_err(|_| Error::PoisonedLock)?;
        assert_eq!(sent[0], RtcpPacket::ConnectivityReport.as_bytes());
        Ok(())
    }

    #[test]
    fn test_close_connection_sets_status_closed() -> Result<(), Error> {
        let mock_socket = MockSocket {
            data_to_receive: vec![],
            sent_data: Arc::new(Mutex::new(Vec::new())),
        };
        let connection_status = Arc::new(RwLock::new(ConnectionStatus::Open));
        let handler = RtcpReportHandler::new(mock_socket, Arc::clone(&connection_status));

        handler.close_connection()?;

        let status = connection_status
            .read()
            .map_err(|_| Error::ConnectionStatusLockFailed)?;
        assert_eq!(*status, ConnectionStatus::Closed);
        Ok(())
    }

    #[test]
    fn test_connection_closes_after_inactivity() -> Result<(), Error> {
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
            .map_err(|_| Error::ConnectionStatusLockFailed)?;
        assert_eq!(*status, ConnectionStatus::Closed);
        Ok(())
    }
}
