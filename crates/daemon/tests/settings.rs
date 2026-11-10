mod common;

use std::time::Duration;

use axum::http::StatusCode;
use common::fake_moonraker::{FakeMoonraker, eventually};
use common::fake_printer::{FakePrinter, fixture};
use common::{cell, json};
use serde_json::{Value, json};
use wobblescope::api::mount;
use wobblescope::config::Settings;
use wobblescope::moonraker::settings::{Variant, parse_settings};
use wobblescope::moonraker::{Moonraker, Timings, spawn};
use wobblescope_session::meta::Mode;

fn real() -> Value {
    fixture("real_settings.json")
}

fn info(app: Option<&str>) -> Value {
    let mut info = fixture("real_printer_info.json");
    if let Some(app) = app {
        info["app"] = json!(app);
    }
    info
}

#[test]
fn the_real_answer_gives_the_effective_values_mainline_sweep_and_a_middle_point() {
    let s = parse_settings(&real(), &info(None)).unwrap();
    assert_eq!(s.variant, Variant::Mainline);
    assert_eq!(s.default_mode, Mode::Sweep);
    assert_eq!(s.settings.freq_start.value, Some(5.0));
    assert_eq!(s.settings.freq_end.value, Some(135.0));
    assert_eq!(s.settings.hz_per_sec.value, Some(1.0));
    assert_eq!(s.settings.accel_per_hz.value, Some(60.0));
    assert_eq!(s.settings.sweeping_accel.value, Some(400.0));
    assert_eq!(s.settings.sweeping_period.value, Some(1.2));
    assert_eq!(s.settings.move_speed.value, Some(50.0));
    assert!(s.settings.freq_end.overridable && s.settings.freq_end.needed);
    assert!(!s.settings.move_speed.needed);
    assert_eq!(s.default_point, Some([150.5, 154.0, 20.0]));
    assert_eq!(s.axis_limits.unwrap().z.min, -2.0);
}

#[test]
fn kalico_with_no_sweeping_period_defaults_to_pulse() {
    let mut query = real();
    query["status"]["configfile"]["settings"]["resonance_tester"]["sweeping_period"] = json!(0.0);
    let s = parse_settings(&query, &info(Some("Kalico"))).unwrap();
    assert_eq!(s.variant, Variant::Kalico);
    assert_eq!(s.default_mode, Mode::Pulse);
}

#[test]
fn an_unknown_app_shows_only_settings_the_config_has() {
    let mut query = real();
    let tester = &mut query["status"]["configfile"]["settings"]["resonance_tester"];
    tester.as_object_mut().unwrap().remove("move_speed");
    let s = parse_settings(&query, &info(Some("SomethingElse"))).unwrap();
    assert_eq!(s.variant, Variant::Unknown);
    assert!(s.settings.freq_end.overridable);
    assert_eq!(s.settings.move_speed.value, None);
    assert!(!s.settings.move_speed.overridable);
}

#[test]
fn a_missing_resonance_tester_or_key_is_unreadable_and_needed() {
    let mut query = real();
    let tester = &mut query["status"]["configfile"]["settings"]["resonance_tester"];
    tester.as_object_mut().unwrap().remove("max_freq");
    let s = parse_settings(&query, &info(None)).unwrap();
    assert_eq!(s.settings.freq_end.value, None);
    assert!(s.settings.freq_end.needed);

    let mut query = real();
    query["status"]["configfile"]["settings"]
        .as_object_mut()
        .unwrap()
        .remove("resonance_tester");
    let s = parse_settings(&query, &info(None)).unwrap();
    assert!(s.settings.hz_per_sec.value.is_none() && s.settings.hz_per_sec.needed);
    assert_eq!(s.default_mode, Mode::Pulse);
}

#[test]
fn an_answer_without_settings_is_an_error_and_without_limits_has_no_point() {
    assert!(parse_settings(&json!({"status": {}}), &info(None)).is_err());
    let mut query = real();
    query["status"].as_object_mut().unwrap().remove("toolhead");
    let s = parse_settings(&query, &info(None)).unwrap();
    assert_eq!((s.axis_limits, s.default_point), (None, None));
}

async fn rig() -> (FakeMoonraker, FakePrinter, Moonraker) {
    let fake = FakeMoonraker::start().await;
    let printer = FakePrinter::new(&fake, "ready", json!({}));
    let real = real();
    printer.set_settings(real["status"]["configfile"]["settings"].clone());
    printer.set_field("toolhead", "axis_minimum", json!([0.0, 0.0, -2.0, 0.0]));
    printer.set_field(
        "toolhead",
        "axis_maximum",
        json!([301.0, 308.0, 300.0, 0.0]),
    );
    let settings = Settings {
        moonraker_url: fake.http_url().parse().unwrap(),
        ..Settings::default()
    };
    let ms = Duration::from_millis;
    let timings = Timings {
        connect: ms(500),
        request: ms(500),
        configfile: ms(800),
        poll_ready: ms(50),
        poll_not_ready: ms(20),
        backoff_start: ms(10),
        backoff_max: ms(40),
    };
    let (moonraker, _) = spawn(&settings, timings, false);
    eventually("a control handle", Duration::from_secs(10), || {
        moonraker.control().is_some()
    })
    .await;
    (fake, printer, moonraker)
}

#[tokio::test]
async fn the_endpoint_answers_with_the_printers_values() {
    let (_fake, _printer, moonraker) = rig().await;
    let app = mount(cell(None), moonraker);
    let (status, body) = json(&app, "GET", "/api/settings").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["variant"], "mainline");
    assert_eq!(body["default_mode"], "Sweep");
    assert_eq!(body["settings"]["accel_per_hz"]["value"], 60.0);
    assert_eq!(body["default_point"], json!([150.5, 154.0, 20.0]));
    assert_eq!(body["segment_overhead_seconds"], json!(15.0));
}

#[tokio::test]
async fn an_unreachable_moonraker_is_a_503_with_a_message() {
    let (_publisher, moonraker) = wobblescope::moonraker::channel();
    let app = mount(cell(None), moonraker);
    let (status, body) = json(&app, "GET", "/api/settings").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "moonraker_unavailable");
    assert!(body["message"].as_str().unwrap().contains("Moonraker"));
}
