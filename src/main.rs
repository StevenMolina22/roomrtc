use std::env;
use std::path::Path;

use room_rtc::config::Config;
use room_rtc::ui::interface::RoomRTCApp;

fn main() -> Result<(), eframe::Error> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} /path/to/room_rtc.conf", args[0]);
        std::process::exit(1);
    }
    
    let config_path = Path::new(&args[1]);

    let config = match Config::load(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!(
                "Failed to load config from {}: {}",
                config_path.display(),
                e
            );
            std::process::exit(1);
        }
    };

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "RoomRTC App",
        options,
        Box::new(|_cc| Ok(Box::new(RoomRTCApp::new(config)))),
    )
}
