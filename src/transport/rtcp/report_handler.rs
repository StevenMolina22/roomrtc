use crate::config::RtcpConfig;
use crate::controller::AppEvent;
use crate::tools::Socket;
use crate::transport::rtcp::RtcpError as Error;
use crate::transport::rtcp::RtcpPacket;
use chrono::{DateTime, Local};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;
use crate::transport::rtcp::metrics::{CallStats, ReceiverStats, SenderStats};

/// RTCP report handler that periodically sends connectivity reports and
/// listens for incoming reports (or goodbye messages) from the peer.
///
/// It spawns a sender thread and a receiver thread; both threads share
/// a connection status flag to indicate when the session is closed.
///
/// # Overview
/// This handler manages the lifecycle of an RTCP reporting session, including:
/// - Initial handshake with the remote peer
/// - Periodic transmission of sender and receiver reports
/// - Reception and processing of incoming reports
/// - Connection state management via an atomic boolean flag
pub struct RtcpReportHandler<S: Socket + Send + Sync + 'static> {
    /// Socket used for sending and receiving RTCP packets.
    socket: Arc<S>,
    /// Shared connection status flag indicating if the session is open.
    connected: Arc<AtomicBool>,
    /// Configuration parameters for RTCP reporting (periods, timeouts, etc).
    config: RtcpConfig,
    /// Local sender statistics for outgoing media (packets sent, bytes sent).
    local_sender_stats: Arc<Mutex<SenderStats>>,
    /// Local receiver statistics for incoming media (packets received, losses, jitter).
    local_receiver_stats: Arc<Mutex<ReceiverStats>>,
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
    pub fn new(
        socket: S,
        connected: Arc<AtomicBool>,
        config: RtcpConfig,
        local_sender_stats: Arc<Mutex<SenderStats>>,
        local_receiver_stats: Arc<Mutex<ReceiverStats>>
    ) -> Self {
        Self {
            socket: Arc::new(socket),
            connected,
            config,
            local_sender_stats,
            local_receiver_stats,
        }
    }

    /// Start the RTCP reporting session by performing a handshake with the remote peer
    /// and spawning background sender and receiver threads.
    ///
    /// This method initiates the RTCP session, then launches threads to periodically send reports and
    /// listen for incoming reports from the peer.
    ///
    /// # Parameters
    /// - `event_tx`: channel for sending application events (stats updates, call end notifications).
    ///
    /// # Returns
    /// `Ok(())` on successful startup, or an `Error` if the handshake or thread spawning fails.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The socket handshake fails
    /// - Thread spawning encounters an OS-level issue
    pub fn start(&self, event_tx: Sender<AppEvent>) -> Result<(), Error> {
        self.connection_handshake()?;
        self.start_report_handler(event_tx)
    }

    /// Spawn the report sender and receiver threads that manage the active session.
    ///
    /// This is an internal helper that sets up both the background sender thread
    /// (which transmits periodic reports) and the receiver thread (which listens
    /// for incoming reports and goodbye messages).
    ///
    /// # Parameters
    /// - `event_tx`: channel for dispatching RTCP-related events to the application.
    ///
    /// # Returns
    /// `Ok(())` if threads are spawned successfully, otherwise an `Error`.
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
            .send(&RtcpPacket::Hello.to_bytes())
            .map_err(|e| Error::SendFailed(e.to_string()))?;

        let mut ready = false;
        println!("HANDSHAKE INIT");
        loop {
            let mut buff = [0u8; 1024];
            match self.socket.recv_from(&mut buff) {
                Ok((size, _addr)) => match RtcpPacket::from_bytes(&buff[..size]) {
                    Some(RtcpPacket::Hello) => {
                        self.socket
                            .send(&RtcpPacket::Ready.to_bytes())
                            .map_err(|e| Error::SendFailed(e.to_string()))?;
                    }
                    Some(RtcpPacket::Ready) => {
                        if ready {
                            break;
                        }
                        ready = true;
                        self.socket
                            .send(&RtcpPacket::Ready.to_bytes())
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

    /// Spawn the background sender thread that periodically sends connectivity reports.
    ///
    /// The sender thread runs in the background and periodically transmits sender reports
    /// and receiver reports according to the configured report period. It retrieves the
    /// latest local statistics and sends them to the remote peer. The thread continues
    /// until the connection flag is set to false, at which point it emits a call-end event.
    ///
    /// # Parameters
    /// - `report_socket`: shared socket for sending RTCP packets to the remote peer.
    /// - `event_tx`: channel for emitting local statistics updates and call-end events.
    fn start_report_sender(&self, report_socket: Arc<S>, event_tx: Sender<AppEvent>) {
        let connected = Arc::clone(&self.connected);
        let report_period_millis = self.config.report_period_millis;
        let local_sender_stats = Arc::clone(&self.local_sender_stats);
        let local_receiver_stats = Arc::clone(&self.local_receiver_stats);
        let event_tx = event_tx.clone();

        thread::spawn(move || {
            loop {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                let s_stats = match local_sender_stats.lock() {
                    Ok(s) => *s,
                    Err(_) => break
                };
                let r_stats = match local_receiver_stats.lock() {
                    Ok(r) => *r,
                    Err(_) => break
                };

                let packet_sr = RtcpPacket::SenderReport(s_stats);
                if report_socket.send(&packet_sr.to_bytes()).is_err() {
                    connected.store(false, Ordering::SeqCst);
                    break;
                }

                let packet_rr = RtcpPacket::ReceiverReport(r_stats);
                if report_socket.send(&packet_rr.to_bytes()).is_err() { break; }

                let local_update = CallStats {
                    local_sender: s_stats,
                    local_receiver: r_stats,
                    ..Default::default()
                };

                let _ = event_tx.send(AppEvent::LocalStatsUpdate(local_update));
                thread::sleep(Duration::from_millis(report_period_millis));
            }

            if connected.load(Ordering::SeqCst) {
                connected.store(false, Ordering::SeqCst);
                let _ = event_tx.send(AppEvent::CallEnded);
            }
        });
    }

    /// Spawn the background receiver thread that listens for incoming RTCP packets.
    ///
    /// The receiver thread monitors incoming RTCP reports and goodbye messages from the
    /// remote peer. It maintains a timeout counter and closes the session if no valid
    /// packets are received within the configured timeout window. The thread will exit
    /// after reaching the retry limit or upon receiving a goodbye packet.
    ///
    /// # Parameters
    /// - `report_socket`: shared socket for receiving RTCP packets from the remote peer.
    /// - `event_tx`: channel for emitting remote statistics updates and call-end events.
    ///
    /// # Returns
    /// `Ok(())` if the receiver thread is spawned successfully, otherwise an `Error`.
    ///
    /// # Errors
    /// Returns an error if socket configuration (setting read timeout) fails.
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
        let max_silence_duration = Duration::from_millis(self.config.receive_limit_millis);

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
                    &event_tx
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

    /// Close the reporting connection by sending a `Goodbye` packet.
    ///
    /// This method signals to the remote peer that the RTCP session is ending
    /// by transmitting a goodbye packet. The sender and receiver threads will
    /// eventually notice the connection closure via the shared status flag.
    ///
    /// # Returns
    /// `Ok(())` on successful transmission, or an `Error` if the socket send fails.
    ///
    /// # Errors
    /// Returns `Error::SendFailed` if the goodbye packet cannot be sent.
    pub fn report_goodbye(&self) -> Result<(), Error> {
        self.socket
            .send(&RtcpPacket::Goodbye.to_bytes())
            .map_err(|e| Error::SendFailed(e.to_string()))?;
        Ok(())
    }
}

/// Try to receive and process a single RTCP report from the socket.
///
/// This function attempts to receive and parse an RTCP packet. If a valid connectivity
/// report (Sender or Receiver Report) is received, it updates the `last_report_time` and
/// emits a remote statistics update event. If a goodbye packet is received, it returns
/// `Error::GoodbyeReceived`. If no valid packet is received within the specified timeout
/// window, it returns `Error::TimedOut`.
///
/// # Parameters
/// - `report_socket`: the socket to receive data from.
/// - `last_report_time`: mutable reference to the timestamp of the last valid report;
///   updated on successful report reception.
/// - `receive_limit`: maximum duration to wait without receiving a valid packet before
///   considering the session timed out.
/// - `event_tx`: channel for sending remote statistics updates to the application.
///
/// # Returns
/// - `Ok(())` if a valid report is processed successfully.
/// - `Err(Error::GoodbyeReceived)` if a goodbye packet is received.
/// - `Err(Error::TimedOut)` if no valid packet arrives within the receive limit.
/// - `Err(Error::UnexpectedMessage)` if an unknown packet type is received.
fn try_receive_report<S: Socket + Send + Sync + 'static>(
    report_socket: &S,
    last_report_time: &mut DateTime<Local>,
    receive_limit: Duration,
    event_tx: &Sender<AppEvent>
) -> Result<(), Error> {

    let mut buf = [0u8; 1024];

    match report_socket.recv_from(&mut buf) {
        Ok((size, _src_addr)) => match RtcpPacket::from_bytes(&buf[..size]) {
            Some(RtcpPacket::SenderReport(s_stats)) => {
                *last_report_time = Local::now();
                let call_stats = CallStats { remote_sender: s_stats, ..Default::default() };
                let _ = event_tx.send(AppEvent::RemoteStatsUpdate(call_stats));
                Ok(())
            }
            Some(RtcpPacket::ReceiverReport(r_stats)) => {
                *last_report_time = Local::now();
                let call_stats = CallStats { remote_receiver: r_stats, ..Default::default() };
                let _ = event_tx.send(AppEvent::RemoteStatsUpdate(call_stats));
                Ok(())
            }
            Some(RtcpPacket::Goodbye) => Err(Error::GoodbyeReceived),
            Some(_) => Err(Error::UnexpectedMessage),
            None => {
                let duration = match chrono::Duration::from_std(receive_limit) {
                    Ok(d) => d,
                    Err(_) => chrono::Duration::seconds(30),
                };
                if Local::now() - *last_report_time > duration
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
