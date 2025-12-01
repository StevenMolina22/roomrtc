use room_rtc::config::Config;
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

    let mut sv = match CentralServer::new(Arc::new(config)) {
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
