use crate::session::sdp::SessionDescriptionProtocol;

pub enum AppEvent {
    CallIncoming(String, SessionDescriptionProtocol),
    CallAccepted(SessionDescriptionProtocol),
    CallRejected,
    CallEnded,
    Error(String),
    FatalError(String),
}
