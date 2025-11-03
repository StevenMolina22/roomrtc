/// Represents the current state of the RTP connection.

#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum ConnectionStatus {
    /// The connection is open and active.
    Open,
    /// The connection is waiting to be started.
    Waiting,
    /// The connection has been closed.
    Closed,
}
