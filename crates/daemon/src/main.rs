use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use wobblescope::api;
use wobblescope::cli::{Cli, Command};
use wobblescope::config;
use wobblescope::moonraker::tls::Tls;
use wobblescope::moonraker::{self, Timings};
use wobblescope::setup;
use wobblescope::startup;
use wobblescope::storage::default_probe;
use wobblescope::web;

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
    let cli = Cli::parse();
    if let Some(Command::Setup(args)) = cli.command {
        return setup::run(args).await;
    }
    init_logging();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Says where the daemon listens and what it trusts, before it binds.
fn log_access(settings: &config::Settings) {
    let list = |items: Vec<String>| {
        if items.is_empty() {
            "none".to_owned()
        } else {
            items.join(", ")
        }
    };
    tracing::info!("listening on {}", settings.listen);
    tracing::info!(
        "trusted_proxies: {}; allowed_hosts: {}",
        list(
            settings
                .trusted_proxies
                .iter()
                .map(ToString::to_string)
                .collect()
        ),
        list(settings.allowed_hosts.clone()),
    );
    tracing::info!(
        "only loopback clients are admitted until Moonraker's trusted_clients and cors_domains \
         are read"
    );
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
    let tls = Tls::new(&settings.moonraker_tls)?;
    for warning in settings.moonraker_warnings() {
        tracing::warn!("{warning}");
    }
    let (moonraker, report) = moonraker::spawn(
        &settings,
        Timings::default(),
        settings.storage_dir.is_none(),
        tls,
    );

    let app = api::mount_configured(
        cell.clone(),
        moonraker.clone(),
        api::ImportGate::default(),
        capture.clone(),
        settings.clone(),
    )
    .fallback(web::static_handler);
    let app = api::guarded(
        app,
        api::AccessGuard::new(
            moonraker.clone(),
            settings.trusted_proxies.clone(),
            settings.allowed_hosts.clone(),
        ),
    );
    log_access(&settings);
    let listener = tokio::net::TcpListener::bind(settings.listen).await?;
    let serve = std::future::IntoFuture::into_future(axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    ));
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
