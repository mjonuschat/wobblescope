mod common;

use axum::http::StatusCode;
use common::{cell, json, open};
use serde_json::json;
use tempfile::TempDir;
use wobblescope::api::mount;
use wobblescope::moonraker::chips::{Availability, Chip, ChipType, SkippedSection};
use wobblescope::moonraker::discovery::{ChipList, Defaults};
use wobblescope::moonraker::status::{
    KlippyState, KlippyStatus, MoonrakerState, MoonrakerStatus, Status,
};
use wobblescope::moonraker::{ChipsSnapshot, channel};

fn chip(section: &str, chip_type: ChipType, sensor: Option<&str>, rate: Option<f64>) -> Chip {
    Chip {
        section: section.to_owned(),
        chip_type,
        sensor: sensor.map(str::to_owned),
        dump_endpoint: "x/dump".to_owned(),
        nominal_rate_hz: rate,
        availability: Availability::Unchecked,
    }
}

#[tokio::test]
async fn a_fresh_daemon_is_connecting_with_waiting_storage_and_no_chips() {
    let (_publisher, moonraker) = channel();
    let app = mount(cell(None), moonraker);
    let (status, body) = json(&app, "GET", "/api/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({
            "moonraker": {"state": "connecting", "message": null, "version": null},
            "klippy": {"state": "unknown", "message": null},
            "storage": {"state": "waiting"},
        })
    );
    let (status, body) = json(&app, "GET", "/api/chips").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["known"], false);
    assert_eq!(body["current"], false);
    assert_eq!(body["chips"], json!([]));
    assert_eq!(body["skipped"], json!([]));
    assert_eq!(body["defaults"], json!({"x": [], "y": []}));
}

#[tokio::test]
async fn the_status_shows_the_published_fields_and_open_storage() {
    let tmp = TempDir::new().unwrap();
    let (storage, _) = open(tmp.path(), u64::MAX / 4);
    let (publisher, moonraker) = channel();
    publisher.set_status(Status {
        moonraker: MoonrakerStatus {
            state: MoonrakerState::Connected,
            message: None,
            version: Some("v0.9.3".to_owned()),
        },
        klippy: KlippyStatus {
            state: KlippyState::Shutdown,
            message: Some("Timer too close".to_owned()),
        },
    });
    let app = mount(cell(Some(storage)), moonraker);
    let (status, body) = json(&app, "GET", "/api/status").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["moonraker"]["state"], "connected");
    assert_eq!(body["moonraker"]["version"], "v0.9.3");
    assert_eq!(body["klippy"]["state"], "shutdown");
    assert_eq!(body["klippy"]["message"], "Timer too close");
    assert_eq!(body["storage"]["state"], "ready");
}

#[tokio::test]
async fn the_chips_show_each_chip_unchecked_the_skipped_reason_and_the_defaults() {
    let (publisher, moonraker) = channel();
    publisher.set_chips(ChipsSnapshot {
        known: true,
        current: true,
        list: ChipList {
            chips: vec![
                chip(
                    "adxl345 nh36",
                    ChipType::Adxl345,
                    Some("nh36"),
                    Some(3200.0),
                ),
                chip("beacon", ChipType::Beacon, None, None),
            ],
            skipped: vec![SkippedSection {
                section: "lis3dh".to_owned(),
                reason: "lis3dh is not supported yet".to_owned(),
            }],
            defaults: Defaults {
                x: vec!["adxl345 nh36".to_owned()],
                y: vec!["adxl345 nh36".to_owned(), "beacon".to_owned()],
            },
        },
    });
    let app = mount(cell(None), moonraker);
    let (status, body) = json(&app, "GET", "/api/chips").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["known"], true);
    assert_eq!(body["current"], true);
    let chips = body["chips"].as_array().unwrap();
    assert_eq!(chips.len(), 2);
    assert_eq!(chips[0]["section"], "adxl345 nh36");
    assert_eq!(chips[0]["chip_type"], "adxl345");
    assert_eq!(chips[0]["sensor"], "nh36");
    assert_eq!(chips[0]["nominal_rate_hz"], 3200.0);
    assert_eq!(chips[0]["availability"], "unchecked");
    assert_eq!(chips[1]["chip_type"], "beacon");
    assert_eq!(chips[1]["sensor"], serde_json::Value::Null);
    assert_eq!(chips[1]["nominal_rate_hz"], serde_json::Value::Null);
    assert_eq!(chips[1]["availability"], "unchecked");
    assert_eq!(body["skipped"][0]["section"], "lis3dh");
    assert_eq!(body["skipped"][0]["reason"], "lis3dh is not supported yet");
    assert_eq!(body["defaults"]["x"], json!(["adxl345 nh36"]));
    assert_eq!(body["defaults"]["y"], json!(["adxl345 nh36", "beacon"]));
}

#[tokio::test]
async fn other_methods_and_unknown_paths_get_the_json_errors() {
    let app = common::app(None);
    let (status, body) = json(&app, "POST", "/api/status").await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(body["error"], "method_not_allowed");
    let (status, body) = json(&app, "GET", "/api/chipz").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn an_unauthorized_status_body_never_holds_the_api_key() {
    // The key is configured but no status field can carry it; the body of an unauthorized
    // status holds only the message the daemon wrote.
    let settings = wobblescope::config::Settings {
        moonraker_api_key: Some("s3cret-key".to_owned()),
        ..wobblescope::config::Settings::default()
    };
    let (publisher, moonraker) = channel();
    publisher.set_status(Status {
        moonraker: MoonrakerStatus {
            state: MoonrakerState::Unauthorized,
            message: Some("add the daemon to trusted_clients or set moonraker_api_key".to_owned()),
            version: None,
        },
        klippy: KlippyStatus {
            state: KlippyState::Unknown,
            message: None,
        },
    });
    let app = mount(cell(None), moonraker);
    let (_, body) = json(&app, "GET", "/api/status").await;
    assert_eq!(body["moonraker"]["state"], "unauthorized");
    assert!(!body.to_string().contains("s3cret-key"));
    assert!(!format!("{settings:?}").contains("s3cret-key"));
}
