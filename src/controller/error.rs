use std::fmt::Display;
use std::net::AddrParseError;

/// Errors that can occur when operating the controller.
///
/// This enum collects the various failure modes the controller may
/// expose when creating sockets, interacting with RTP senders and
/// receivers, managing threads, or working with internal maps and
/// locks. Variants carry contextual error information where
/// available.
#[derive(Debug, Eq, PartialEq)]
pub enum ControllerError {
    /// Failed to parse a socket address.
    ParsingSocketAddressError(AddrParseError),

    /// Failed to bind a socket to the requested address.
    BindingAddressError(String),

    /// General socket connection error.
    ConnectionSocketError(String),

    /// Error cloning a UDP socket for use across threads.
    CloningSocketError(String),

    /// Error creating an RTP sender.
    RtpSenderError(String),

    /// Error creating an RTP receiver.
    RtpReceiverError(String),

    /// A poisoned mutex or lock was encountered.
    PoisonedLock,

    /// Generic mapping/lookup error carrying a message.
    MapError(String),

    /// No RTP sender has been built yet when one was expected.
    EmptyRTPSenderError,

    /// Failed to join a spawned thread.
    JoinThreadError,

    /// Connection has not been started when an operation required it.
    ConnectionNotStarted,

    /// The connection was closed while an operation was in progress.
    ConnectionClosed,
}

/// Errors produced by worker threads.
///
/// Threads can return either recoverable errors or fatal errors which should cause immediate
/// termination of the worker's responsibilities.
pub enum ThreadsError {
    /// A recoverable error; carries a message explaining the condition.
    Recoverable(String),
    /// A fatal error reported by the thread.
    Fatal(String),
}

impl Display for ControllerError {
    /// Format a readable representation of the controller error.
    ///
    /// These messages include brief context and the underlying error details where present.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParsingSocketAddressError(e) => write!(
                f,
                "Error: \"Failed to parse socket address\". Details: {e}"
            ),
            Self::BindingAddressError(e) => {
                write!(f, "Error: \"Failed to bind socket\". Details: {e}")
            }
            Self::ConnectionSocketError(e) => {
                write!(f, "Error: \"Failed to connect\". Details: {e}")
            }
            Self::CloningSocketError(e) => {
                write!(f, "Error: \"Failed to clone UDP socket\" Details: {e}")
            }
            Self::RtpSenderError(e) => {
                write!(f, "Error: \"Failed to create RTP sender\". Details: {e}")
            }
            Self::RtpReceiverError(e) => write!(
                f,
                "Error: \"Failed to create RTP receiver\". Details: {e}"
            ),
            Self::PoisonedLock => write!(f, "Error: \"Poisoned lock\""),
            Self::MapError(e) => write!(f, "{e}"),
            Self::EmptyRTPSenderError => write!(f, "Error: \"there is no RTP sender built yet\""),
            Self::JoinThreadError => write!(f, "Error: \"Failed to join thread\""),
            Self::ConnectionNotStarted => write!(f, "Error: \"Connection not started\""),
            Self::ConnectionClosed => write!(f, "Error: \"Connection closed\""),
        }
    }
}
