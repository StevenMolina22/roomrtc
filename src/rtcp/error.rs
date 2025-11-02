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
}

impl Display for RtcpError {
    /// Format the RTCP error as a short human-readable string.
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RtcpError::PoisonedLock => write!(f, "Error: Poisoned lock"),
            RtcpError::SocketCloneFailed => write!(f, "Error: \"Failed to clone socket\""),
            RtcpError::InvalidRTCPPacket => write!(f, "Error: \"Invalid or corrupted RTCP packet\""),
            RtcpError::SocketConfigFailed => write!(f, "Error: \"Failed to configure socket\""),
            RtcpError::GoodbyeReceived => write!(f, "Error: \"Goodbye\""),
            RtcpError::TimedOut => write!(f, "Error: \"Report receiver timed out\""),
            RtcpError::ConnectionStatusLockFailed => write!(f, "Error: \"Failed to acquire connection status lock\""),
            RtcpError::InvalidConfigDuration => write!(f, "Error: \"Invalid configuration duration\""),
        }
    }
}

impl std::error::Error for RtcpError {}
