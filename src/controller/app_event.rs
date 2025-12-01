use crate::session::sdp::SessionDescriptionProtocol;

pub enum AppEvent {
    CallIncoming(String, SessionDescriptionProtocol),
    CallEnded,
    Error(String),
    FatalError(String),
}
