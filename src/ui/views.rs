#[derive(Default, PartialEq)]
pub enum View {
    #[default]
    Menu,
    Call,
    Connection {
        our_offer: String,
        remote_sdp: String,
        our_answer: Option<String>,
    }
}