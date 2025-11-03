/// Represents the different UI screens/views the application can show.
///
/// This enum models the high-level states
/// used by the UI layer: main menu, active call view, connection
/// information, a waiting-for-peer screen and a generic error view.
#[derive(Default, PartialEq, Debug, Clone, Copy)]
pub enum View {
    /// The main menu / initial screen.
    #[default]
    Menu,

    /// The in-call view shown when a call is active.
    Call,

    /// A view that shows connection details or connection status.
    Connection,

    /// Generic error view to show unrecoverable or displayable errors.
    Error,
}
