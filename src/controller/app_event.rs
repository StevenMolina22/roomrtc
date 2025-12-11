use crate::session::sdp::SessionDescriptionProtocol;

pub enum AppEvent {
    CallIncoming(String, SessionDescriptionProtocol),
    CallAccepted(SessionDescriptionProtocol, String, String),
    CallRejected,
    CallEnded,
    Error(String),
    FatalError(String),
}
