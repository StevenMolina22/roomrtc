use std::fmt::{Display, Formatter};

/// Error type used by the RTCP module.
///
/// Represents the possible failures encountered while handling RTCP
/// reports and managing the reporting background threads.
#[derive(PartialEq, Eq, Debug)]
pub enum RtcpError {
    /// Lock was poisoned while accessing shared synchronization primitives.
    PoisonedLock,

    /// Cloning of the underlying socket failed.
    SocketCloneFailed,

    /// Received RTCP packet is malformed or not recognized.
    InvalidRTCPPacket,

    /// Failed to configure the socket (timeout, options, etc.).
    SocketConfigFailed,

    /// A RTCP Goodbye packet was received indicating shutdown.
    GoodbyeReceived,

    /// The report receiver timed out waiting for reports.
    TimedOut,

    /// Failed to acquire the connection status lock.
    ConnectionStatusLockFailed,

    /// Provided duration cannot be converted to the expected type.
    InvalidConfigDuration,

    /// Sending an RTCP packet failed.
    SendFailed(String),

    /// Receiving an RTCP packet failed.
    ReceiveFailed(String),

    /// An unexpected RTCP message was received.
    UnexpectedMessage,
}

impl Display for RtcpError {
    /// Format the RTCP error as a short string.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PoisonedLock => write!(f, "Error: Poisoned lock"),
            Self::InvalidRTCPPacket => {
                write!(f, "Error: \"Invalid or corrupted RTCP packet\"")
            }
            Self::SocketCloneFailed => write!(f, "Error: \"Failed to clone socket\""),
            Self::SocketConfigFailed => write!(f, "Error: \"Failed to configure socket\""),
            Self::GoodbyeReceived => write!(f, "Error: \"Goodbye\""),
            Self::TimedOut => write!(f, "Error: \"Report receiver timed out\""),
            Self::ConnectionStatusLockFailed => {
                write!(f, "Error: \"Failed to acquire connection status lock\"")
            }
            Self::InvalidConfigDuration => {
                write!(f, "Error: \"Invalid configuration duration\"")
            }
            Self::SendFailed(e) => {
                write!(f, "Error: \"Failed to send RTCP packet. Details: {e}\"")
            }
            Self::ReceiveFailed(e) => {
                write!(f, "Error: \"Failed to receive RTCP packet. Details: {e}\"")
            }
            Self::UnexpectedMessage => write!(f, "Error: Unexpected message"),
        }
    }
}

impl std::error::Error for RtcpError {}
