use std::path::PathBuf;
use std::process::ExitCode;

use wobblescope::api;
use wobblescope::config;
use wobblescope::moonraker::{self, Timings};
use wobblescope::startup;
use wobblescope::storage::default_probe;
use wobblescope::web;

const LISTEN_ADDR: &str = "127.0.0.1:7126";
const CONFIG_ENV: &str = "WOBBLESCOPE_CONFIG";

/// Logs to stderr, where systemd's journal picks it up and adds its own timestamp. The level is
/// `info` unless `RUST_LOG` says otherwise.
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

#[tokio::main]
async fn main() -> ExitCode {
    init_logging();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = std::env::var_os(CONFIG_ENV).map(PathBuf::from);
    let settings = config::load(config_path.as_deref())?;

    // With `storage_dir` set, storage opens before the listener binds. Without it, the
    // coordinator opens storage once Moonraker names the config root.
    let cell = startup::new_cell();
    let capture = api::CaptureState::new(settings.max_segment_seconds);
    if let Some(dir) = settings.storage_dir.as_deref() {
        let storage = startup::open_storage(dir, &settings, default_probe(), Some(&capture.slot))?;
        let _ = cell.set(storage);
    }
    let (moonraker, report) = moonraker::spawn(
        &settings,
        Timings::default(),
        settings.storage_dir.is_none(),
    );

    let app = api::mount_capture(
        cell.clone(),
        moonraker.clone(),
        api::ImportGate::default(),
        capture.clone(),
    )
    .fallback(web::static_handler);
    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR).await?;
    let serve = std::future::IntoFuture::into_future(axum::serve(listener, app));
    tokio::pin!(serve);
    if let Some(report) = report {
        let coordinator = startup::resolve_storage(
            report,
            settings,
            default_probe(),
            cell,
            Some(capture.slot.clone()),
        );
        tokio::select! {
            served = &mut serve => return Ok(served?),
            opened = coordinator => opened?,
        }
    }
    serve.await?;
    drop(moonraker);
    Ok(())
}
