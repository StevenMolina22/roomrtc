use crate::session::sdp::SessionDescriptionProtocol;
use crate::transport::rtcp::metrics::CallStats;

pub enum AppEvent {
    FullServerError,
    CallIncoming(String, SessionDescriptionProtocol),
    CallAccepted(SessionDescriptionProtocol, String, String),
    CallRejected,
    CallEnded,
    Error(String),
    FatalError(String),
    LocalStatsUpdate(CallStats),
    RemoteStatsUpdate(CallStats),
}
