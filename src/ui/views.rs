#[derive(Default, PartialEq)]
pub enum View {
    #[default]
    Menu,
    Connecting {
        // Our own offer (if we are offerer)
        our_offer: String,
        // The remote's SDP (offer or answer)
        remote_sdp: String,
        // Our answer (if we are answerer)
        our_answer: Option<String>,
    },
    Call,
}
