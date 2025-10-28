/// Represents the current state of the RTP connection.

#[derive(Eq, PartialEq, Debug)]
pub enum ConnectionStatus {
    /// The connection is open and active.
    Open,
    /// The connection has been closed.
    Closed,
}
