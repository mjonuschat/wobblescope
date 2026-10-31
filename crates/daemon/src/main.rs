use std::path::PathBuf;
use std::process::ExitCode;

use axum::Router;
use wobblescope::config;
use wobblescope::storage::{Storage, default_probe};
use wobblescope::web;

const LISTEN_ADDR: &str = "127.0.0.1:7126";
const CONFIG_ENV: &str = "WOBBLESCOPE_CONFIG";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wobblescope: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = std::env::var_os(CONFIG_ENV).map(PathBuf::from);
    let settings = config::load(config_path.as_deref())?;

    // TEMPORARY: the default data directory comes from Moonraker's file roots, and there is no
    // Moonraker client yet. Until discovery exists, storage needs `storage_dir` in the config.
    let _storage = match settings.storage_dir.as_deref() {
        Some(dir) => {
            let (storage, report) = Storage::open(dir, &settings, default_probe())?;
            for line in report.lines() {
                eprintln!("storage: {line}");
            }
            Some(storage)
        }
        None => {
            eprintln!(
                "storage disabled: set storage_dir in the file named by {CONFIG_ENV} \
                 (the default directory cannot be derived until Moonraker discovery exists)"
            );
            None
        }
    };

    let app = Router::new().fallback(web::static_handler);
    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
