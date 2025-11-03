use std::env;
use std::path::Path;

use roomrtc::config::Config;
use roomrtc::ui::interface::RoomRTCApp;

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} /path/to/roomrtc.conf", args[0]);
        std::process::exit(1);
    }
    let config_path = &args[1];

    let config = Config::load(Path::new(config_path))
        .expect("Failed to load configuration file. Make sure it exists and is valid.");

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "RoomRTC App",
        options,
        Box::new(|_cc| Ok(Box::new(RoomRTCApp::new(config)))),
    )
}
