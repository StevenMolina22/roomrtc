use crate::config::RtcpConfig;
use crate::controller::AppEvent;
use crate::tools::Socket;
use crate::transport::rtcp::RtcpError as Error;
use crate::transport::rtcp::RtcpPacket;
use chrono::{DateTime, Local, TimeDelta};
use std::io::ErrorKind;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

/// RTCP report handler that periodically sends connectivity reports and
/// listens for incoming reports (or goodbye messages) from the peer.
///
/// It spawns a sender thread and a receiver thread; both threads share
/// a connection status flag to indicate when the session is closed.
pub struct RtcpReportHandler<S: Socket + Send + Sync + 'static> {
    socket: Arc<S>,
    connected: Arc<AtomicBool>,
    config: RtcpConfig,
    local_ssrc: u32,
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
    /// - `local_ssrc`: the Synchronization Source identifier for this peer,
    ///   used to sign outgoing packets for SRTCP context lookup.
    ///
    /// # Returns
    /// A configured `RtcpReportHandler` that is ready to `start()`.
    pub fn new(socket: S, connected: Arc<AtomicBool>, config: RtcpConfig, local_ssrc: u32) -> Self {
        Self {
            socket: Arc::new(socket),
            connected,
            config,
            local_ssrc,
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
        self.socket
            .send(&RtcpPacket::Hello(self.local_ssrc).to_bytes())
            .map_err(|e| Error::SendFailed(e.to_string()))?;

        let mut ready = false;

        loop {
            let packet = if ready {
                RtcpPacket::Ready(self.local_ssrc)
            } else {
                RtcpPacket::Hello(self.local_ssrc)
            };

            self.socket
                .send(&packet.to_bytes())
                .map_err(|e| Error::SendFailed(e.to_string()))?;

            let mut buff = [0u8; 1024];
            match self.socket.recv_from(&mut buff) {
                Ok((size, _addr)) => match RtcpPacket::from_bytes(&buff[..size]) {
                    Some(RtcpPacket::Hello(_)) => {
                        self.socket
                            .send(&RtcpPacket::Ready(self.local_ssrc).to_bytes())
                            .map_err(|e| Error::SendFailed(e.to_string()))?;
                    }
                    Some(RtcpPacket::Ready(_)) => {
                        if ready {
                            break;
                        }
                        ready = true;
                        self.socket
                            .send(&RtcpPacket::Ready(self.local_ssrc).to_bytes())
                            .map_err(|e| Error::SendFailed(e.to_string()))?;
                        self.connected.store(true, Ordering::SeqCst);
                    }
                    Some(_) => return Err(Error::UnexpectedMessage),
                    None => continue,
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
        let local_ssrc = self.local_ssrc;

        thread::spawn(move || {
            loop {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                if report_socket.send(&RtcpPacket::ConnectivityReport(local_ssrc).to_bytes()).is_err()
                {
                    break;
                }
                thread::sleep(Duration::from_millis(report_period_millis));
            }

            if connected.load(Ordering::SeqCst) {
                connected.store(false, Ordering::SeqCst);
                let _ = event_tx.send(AppEvent::CallEnded);
            }
        });
    }

    /// Spawn the background receiver thread which listens for incoming
    /// RTCP-style packets and updates the `connection_status` accordingly.
    ///
    /// The receiver runs with a read timeout configured to `REPORT_RECEIVE_LIMIT`
    /// and uses `try_receive_report` to parse and handle incoming data.
    fn start_report_receiver(
        &self,
        report_socket: Arc<S>,
        event_tx: Sender<AppEvent>,
    ) -> Result<(), Error> {
        report_socket
            .set_read_timeout(Some(Duration::from_millis(
                self.config.receive_limit_millis,
            )))
            .map_err(|_| Error::SocketConfigFailed)?;

        let connected = Arc::clone(&self.connected);
        let retry_limit = self.config.retry_limit;
        let max_silence_duration =
            chrono::Duration::milliseconds(self.config.receive_limit_millis as i64);

        thread::spawn(move || {
            let mut last_valid_packet_time = Local::now();
            let mut timeouts_triggered = 0;

            while timeouts_triggered < retry_limit {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                match try_receive_report(
                    &*report_socket,
                    &mut last_valid_packet_time,
                    max_silence_duration,
                ) {
                    Ok(()) => timeouts_triggered = 0,
                    Err(Error::GoodbyeReceived) => {
                        break;
                    }
                    Err(_) => timeouts_triggered += 1,
                }
            }

            if connected.load(Ordering::SeqCst) {
                connected.store(false, Ordering::SeqCst);
                let _ = event_tx.send(AppEvent::CallEnded);
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
        self.socket
            .send(&RtcpPacket::Goodbye(self.local_ssrc).to_bytes())
            .map_err(|e| Error::SendFailed(e.to_string()))?;
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
    receive_limit: TimeDelta,
) -> Result<(), Error> {
    let mut buf = [0u8; 1024];
    loop {
        match report_socket.recv_from(&mut buf) {
            Ok((size, _src_addr)) => match RtcpPacket::from_bytes(&buf[..size]) {
                Some(RtcpPacket::ConnectivityReport(_)) => {
                    *last_report_time = Local::now();
                    return Ok(());
                }
                Some(RtcpPacket::Goodbye(_)) => return Err(Error::GoodbyeReceived),
                _ => continue,
            },
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                return if Local::now() - *last_report_time > receive_limit {
                    Err(Error::TimedOut)
                } else {
                    Ok(())
                };
            }
            Err(e) => return Err(Error::ReceiveFailed(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RtcpConfig;
    use crate::tools::MockSocket;
    use crate::transport::rtcp::RtcpPacket;
    use std::sync::{Arc, Mutex};
    

    const TEST_SSRC: u32 = 0x1234_5678;

    fn test_config() -> RtcpConfig {
        RtcpConfig {
            report_period_millis: 50,
            receive_limit_millis: 80,
            retry_limit: 2,
        }
    }

    #[test]
    fn test_report_goodbye_sends_goodbye() -> Result<(), Error> {
        let sent_data = Arc::new(Mutex::new(Vec::new()));

        let mock_socket = MockSocket {
            data_to_receive: Arc::new(Mutex::new(vec![])),
            sent_data: sent_data.clone(),
        };

        let connected = Arc::new(AtomicBool::new(true));
        let handler =
            RtcpReportHandler::new(mock_socket, connected, test_config(), TEST_SSRC);

        handler.report_goodbye()?;

        let sent = sent_data.lock().unwrap();
        assert!(
            sent.iter()
                .any(|msg| RtcpPacket::from_bytes(msg) == Some(RtcpPacket::Goodbye(TEST_SSRC))),
            "No se envió el Goodbye"
        );

        Ok(())
    }
}
