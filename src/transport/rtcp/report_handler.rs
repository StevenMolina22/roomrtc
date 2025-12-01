use crate::config::RtcpConfig;
use crate::tools::Socket;
use crate::transport::rtcp::RtcpError as Error;
use crate::transport::rtcp::RtcpPacket;
use chrono::{DateTime, Local};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;
use crate::controller::AppEvent;

/// RTCP report handler that periodically sends connectivity reports and
/// listens for incoming reports (or goodbye messages) from the peer.
///
/// It spawns a sender thread and a receiver thread; both threads share
/// a connection status flag to indicate when the session is closed.
pub struct RtcpReportHandler<S: Socket + Send + Sync + 'static> {
    socket: Arc<S>,
    connected: Arc<AtomicBool>,
    config: RtcpConfig,
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
    pub fn new(socket: S, connected: Arc<AtomicBool>, config: RtcpConfig) -> Self {
        Self {
            socket: Arc::new(socket),
            connected,
            config,
        }
    }
    pub fn start(&self, event_tx: Sender<AppEvent>) -> Result<(), Error> {
        self.connection_handshake()?;
        self.start_report_handler(event_tx)
    }

    fn start_report_handler(&self, event_tx: Sender<AppEvent>) -> Result<(), Error> {
        let sender_socket = Arc::clone(&self.socket);
        let receiver_socket = Arc::clone(&self.socket);

        self.start_report_receiver(receiver_socket, event_tx.clone())?;
        self.start_report_sender(sender_socket, event_tx);
        Ok(())
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
    pub fn connection_handshake(&self) -> Result<(), Error> {
        if let Err(e) = self.socket.send(RtcpPacket::Hello.as_bytes()) {
            eprintln!("{e}");
        }

        let mut ready = false;

        loop {
            let mut buff = [0u8; 1024];
            match self.socket.recv_from(&mut buff) {
                Ok((size, _addr)) => match RtcpPacket::from_bytes(&buff[..size]) {
                    Some(RtcpPacket::Hello) => {
                        self.socket
                            .send(RtcpPacket::Ready.as_bytes())
                            .map_err(|e| Error::SendFailed(e.to_string()))?;
                    }
                    Some(RtcpPacket::Ready) => {
                        if ready {
                            break;
                        }
                        ready = true;
                        self.socket
                            .send(RtcpPacket::Ready.as_bytes())
                            .map_err(|e| Error::SendFailed(e.to_string()))?;
                        self.connected.store(true, Ordering::SeqCst);
                    }
                    Some(_) => return Err(Error::UnexpectedMessage),
                    None => {}
                },
                Err(e) => return Err(Error::ReceiveFailed(e.to_string())),
            }
        }

        Ok(())
    }

    /// Spawn the background sender thread that periodically sends
    /// connectivity reports until the connection is closed.
    fn start_report_sender(&self, report_socket: Arc<S>, event_tx: Sender<AppEvent>) {
        let connected = Arc::clone(&self.connected);
        let report_period_millis = self.config.report_period_millis;

        thread::spawn(move || {
            loop {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                if report_socket
                    .send(RtcpPacket::ConnectivityReport.as_bytes())
                    .is_err()
                {
                    connected.store(false, Ordering::SeqCst);
                    break;
                }

                thread::sleep(Duration::from_millis(report_period_millis));
            }

            if connected.load(Ordering::SeqCst) {
                connected.store(false, Ordering::SeqCst);
                event_tx.send(AppEvent::CallEnded);
            }
        });
    }

    /// Spawn the background receiver thread which listens for incoming
    /// RTCP-style packets and updates the `connection_status` accordingly.
    ///
    /// The receiver runs with a read timeout configured to `REPORT_RECEIVE_LIMIT`
    /// and uses `try_receive_report` to parse and handle incoming data.
    fn start_report_receiver(&self, report_socket: Arc<S>, event_tx: Sender<AppEvent>) -> Result<(), Error> {
        report_socket
            .set_read_timeout(Some(Duration::from_millis(
                self.config.receive_limit_millis,
            )))
            .map_err(|_| Error::SocketConfigFailed)?;

        let connected = Arc::clone(&self.connected);
        let retry_limit = self.config.retry_limit;
        let receive_limit = Duration::from_millis(self.config.receive_limit_millis);

        thread::spawn(move || {
            let mut last_report_time = Local::now();
            let mut retries = 0;

            while retries < retry_limit {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                match try_receive_report(&*report_socket, &mut last_report_time, receive_limit) {
                    Ok(()) => retries = 0,
                    Err(Error::GoodbyeReceived) => {
                        println!("Goodbye recibido!");
                        retries = retry_limit;
                    }
                    Err(_) => retries += 1,
                }
            }

            if connected.load(Ordering::SeqCst) {
                connected.store(false, Ordering::SeqCst);
                event_tx.send(AppEvent::CallEnded);
            }
        });

        Ok(())
    }

    /// Close the reporting connection by sending a `Goodbye` packet
    /// and updating the `connection_status` to `Closed`.
    ///
    /// # Returns
    /// A result indicating success or failure.
    pub fn report_goodbye(&self) -> Result<(), Error> {
        self.socket.send(RtcpPacket::Goodbye.as_bytes()).map_err(|e| Error::SendFailed(e.to_string()))?;
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
    receive_limit: Duration,
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
                    > chrono::Duration::from_std(receive_limit)
                    .unwrap_or_else(|_| chrono::Duration::seconds(30))
                {
                    Err(Error::TimedOut)
                } else {
                    Ok(())
                }
            }
        },
        Err(_) => {
            if Local::now() - *last_report_time
                > chrono::Duration::from_std(receive_limit)
                .map_err(|_| Error::InvalidConfigDuration)?
            {
                Err(Error::TimedOut)
            } else {
                Ok(())
            }
        }
    }
}