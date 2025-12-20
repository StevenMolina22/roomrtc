use room_rtc::config::Config;
use room_rtc::logger::Logger;
use room_rtc::ui::interface::RoomRTCApp;
use std::env;
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;

fn main() -> Result<(), eframe::Error> {
    if rustls::crypto::ring::default_provider().install_default().is_err() {
        println!("error initializing client");
        std::process::exit(1);
    }

    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        std::process::exit(1);
    }

    let config_path = Path::new(&args[1]);
    let server_addr = match SocketAddr::from_str(&args[2]) {
        Ok(server_addr) => server_addr,
        Err(_) => std::process::exit(1),
    };

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

    let log_path = "room_rtc.log";
    let logger = match Logger::new(log_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to initialize logger: {e}");
            std::process::exit(1);
        }
    };

    logger.info("Application starting...");

    let options = eframe::NativeOptions::default();
    // TODO! Pass the logger into the app
    eframe::run_native(
        "RoomRTC App",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(RoomRTCApp::new(config, server_addr, logger)))
        }),
    )
}
