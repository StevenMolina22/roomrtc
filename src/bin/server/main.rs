use room_rtc::config::Config;
use room_rtc::logger::Logger;
use room_rtc::server::CentralServer;
use std::env;
use std::path::Path;
use std::sync::Arc;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
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

    // Logger
    let log_path = "room_rtc.server.log"; // Use a dedicated path
    let logger = match Logger::new(log_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to initialize logger: {e}");
            std::process::exit(1);
        }
    };
    logger.info("Central Server starting up...");

    let sv_config = Arc::new(config);

    let mut sv = match CentralServer::new(sv_config, logger.context("CentralServer")) {
        Ok(sv) => sv,
        Err(e) => {
            eprintln!("Failed to start server: {e}");
            std::process::exit(1);
        }
    };
    if sv.start().is_err() {
        std::process::exit(1);
    }
}
