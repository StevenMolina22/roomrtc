use crate::session::sdp::SessionDescriptionProtocol;

/// Represents the different UI screens/views the application can show.
///
/// This enum models the high-level states
/// used by the UI layer: main menu, active call view, connection
/// information, a waiting-for-peer screen and a generic error view.
#[derive(Default, PartialEq, Eq, Debug, Clone)]
pub enum View {
    #[default]
    Welcome,

    SignUp,

    LogIn,

    CallHub,

    Calling(String),

    CallIncoming(String, SessionDescriptionProtocol),

    CallEnded,

    /// The in-call view shown when a call is active.
    Call(String, String),

    /// Generic error view to show unrecoverable or displayable errors.
    Error,

    FatalError,
    
    FullServer
}
