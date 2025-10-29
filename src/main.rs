use roomrtc::{ui::interface::RoomRTCApp};

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "RoomRTC App",
        options,
        Box::new(|_cc| Ok(Box::new(RoomRTCApp::new()))),
    )
}