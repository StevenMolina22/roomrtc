use crate::session::sdp::SessionDescriptionProtocol;
use crate::transport::rtcp::metrics::CallStats;

/// Events that can occur in the application and are sent to the UI.
///
/// This enum represents significant events during the application's lifecycle,
/// particularly related to call management and error handling. Events are typically
/// sent through a channel to the UI layer for processing and user notification.
///
/// # Variants
///
/// - `CallIncoming`: An incoming call is received.
/// - `CallAccepted`: A call has been accepted by the peer.
/// - `CallRejected`: An outgoing call was rejected.
/// - `CallEnded`: An active call has ended.
/// - `Error`: A recoverable error occurred.
/// - `FatalError`: A fatal error occurred that may require user intervention.
pub enum AppEvent {
    FullServerError,

    /// Incoming call notification with caller username and SDP offer.
    ///
    /// Contains the username of the caller and the SDP offer for establishing the call.
    CallIncoming(String, SessionDescriptionProtocol),

    /// Call acceptance notification with SDP answer and user information.
    ///
    /// Contains the SDP answer, the username accepting the call, and the username of the caller.
    CallAccepted(SessionDescriptionProtocol, String, String),

    /// Incoming call was rejected by the peer.
    CallRejected,

    /// Active call has ended.
    CallEnded,

    /// Recoverable error occurred during operation.
    Error(String),
    FatalError,
    LocalStatsUpdate(CallStats),
    RemoteStatsUpdate(CallStats),
}
