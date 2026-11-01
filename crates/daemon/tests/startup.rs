mod common;

use std::path::PathBuf;
use std::time::Duration;

use axum::http::StatusCode;
use common::fake_moonraker::{FakeMoonraker, Script, eventually};
use common::{cell, json, open, probe, settings};
use serde_json::{Value, json};
use tempfile::TempDir;
use wobblescope::config::Settings;
use wobblescope::moonraker::{Timings, spawn};
use wobblescope::startup::{StartupError, new_cell, resolve_storage};

const DEADLINE: Duration = Duration::from_secs(10);

fn fast() -> Timings {
    Timings {
        connect: Duration::from_millis(500),
        request: Duration::from_millis(200),
        configfile: Duration::from_millis(800),
        poll_ready: Duration::from_millis(20),
        poll_not_ready: Duration::from_millis(10),
        backoff_start: Duration::from_millis(10),
        backoff_max: Duration::from_millis(40),
    }
}

fn settings_for(fake: &FakeMoonraker) -> Settings {
    Settings {
        moonraker_url: fake.http_url().parse().unwrap(),
        ..settings(u64::MAX / 4, 0)
    }
}

fn roots(config: &str) -> Value {
    json!([{"name": "config", "path": config, "permissions": "rw"}])
}

/// Starts the supervisor and the coordinator against `fake`, as `main` does without
/// `storage_dir`.
fn start(
    fake: &FakeMoonraker,
    cell: &wobblescope::startup::StorageCell,
) -> (
    wobblescope::moonraker::Moonraker,
    tokio::task::JoinHandle<Result<(), StartupError>>,
) {
    let settings = settings_for(fake);
    let (moonraker, report) = spawn(&settings, fast(), true);
    let task = tokio::spawn(resolve_storage(
        report.unwrap(),
        settings,
        probe(u64::MAX / 2),
        cell.clone(),
    ));
    (moonraker, task)
}

async fn failure_of(config: Value) -> String {
    let fake = FakeMoonraker::start().await;
    fake.on("server.files.roots", Script::Result(config));
    let cell = new_cell();
    let (_moonraker, task) = start(&fake, &cell);
    let err = tokio::time::timeout(DEADLINE, task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(cell.get().is_none(), "storage must stay closed");
    err.to_string()
}

#[tokio::test]
async fn storage_opens_beside_the_config_root_once_moonraker_answers() {
    let tmp = TempDir::new().unwrap();
    let config = tmp.path().join("printer_data/config");
    let fake = FakeMoonraker::start().await;
    fake.set_accepting(false);
    fake.on(
        "server.files.roots",
        Script::Result(roots(config.to_str().unwrap())),
    );
    let cell = new_cell();
    let app = wobblescope::api::mount(cell.clone());
    let (_moonraker, task) = start(&fake, &cell);

    eventually("a connection attempt", DEADLINE, || {
        !fake.attempts().is_empty()
    })
    .await;
    let (status, body) = json(&app, "GET", "/api/sessions").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "storage_unavailable");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("waiting for Moonraker")
    );

    fake.set_accepting(true);
    tokio::time::timeout(DEADLINE, task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let data = tmp.path().join("printer_data/wobblescope");
    assert!(data.join("sessions").is_dir());
    let (status, body) = json(&app, "GET", "/api/sessions").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["sessions"], json!([]));
}

#[tokio::test]
async fn roots_without_a_config_entry_are_fatal_and_name_the_config_root() {
    let err = failure_of(json!([{"name": "gcodes", "path": "/g", "permissions": "rw"}])).await;
    assert!(err.contains("config root"), "{err}");
}

#[tokio::test]
async fn the_root_slash_is_fatal_because_it_has_no_parent() {
    let err = failure_of(roots("/")).await;
    assert!(err.contains("no parent"), "{err}");
}

#[tokio::test]
async fn a_relative_config_root_is_fatal() {
    let err = failure_of(roots("printer_data/config")).await;
    assert!(err.contains("not an absolute path"), "{err}");
}

#[tokio::test]
async fn a_damaged_pins_file_is_fatal_and_carries_the_storage_error() {
    let tmp = TempDir::new().unwrap();
    let data = tmp.path().join("printer_data/wobblescope");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("pins.json"), "not json").unwrap();
    let err = failure_of(roots(
        tmp.path().join("printer_data/config").to_str().unwrap(),
    ))
    .await;
    assert!(err.contains("pins.json"), "{err}");
    assert!(err.contains(data.to_str().unwrap()), "{err}");
}

#[tokio::test]
async fn with_storage_dir_set_sessions_answer_at_once_and_roots_are_never_asked() {
    let tmp = TempDir::new().unwrap();
    let fake = FakeMoonraker::start().await;
    fake.set_accepting(false);
    let settings = Settings {
        storage_dir: Some(PathBuf::from(tmp.path())),
        ..settings_for(&fake)
    };
    let (storage, _) = open(tmp.path(), u64::MAX / 4);
    let app = wobblescope::api::mount(cell(Some(storage)));
    let (_moonraker, report) = spawn(&settings, fast(), settings.storage_dir.is_none());
    assert!(report.is_none());
    let (status, _) = json(&app, "GET", "/api/sessions").await;
    assert_eq!(status, StatusCode::OK);
    fake.set_accepting(true);
    eventually("a websocket connection", DEADLINE, || {
        fake.connections() > 0
    })
    .await;
    assert!(fake.requests_for("server.files.roots").is_empty());
}
