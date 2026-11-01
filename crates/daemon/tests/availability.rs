mod common;

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use common::fake_bridge::{FakeBridge, Subscribe};
use common::fake_moonraker::{FakeMoonraker, eventually};
use common::fake_printer::FakePrinter;
use common::{cell, json_post};
use serde_json::{Value, json};
use wobblescope::api::mount;
use wobblescope::capture::availability::{open_chips, rate_from_times};
use wobblescope::config::Settings;
use wobblescope::moonraker::chips::Availability;
use wobblescope::moonraker::status::KlippyState;
use wobblescope::moonraker::{Moonraker, Timings, spawn};

const DEADLINE: Duration = Duration::from_secs(10);

fn fast() -> Timings {
    let ms = Duration::from_millis;
    Timings {
        connect: ms(500),
        request: ms(500),
        configfile: ms(800),
        poll_ready: ms(50),
        poll_not_ready: ms(20),
        backoff_start: ms(10),
        backoff_max: ms(40),
    }
}

fn two_chips() -> Value {
    json!({
        "adxl345": {"cs_pin": "PA4"},
        "lis2dw ebb36": {"cs_pin": "ebb36: PA4"},
        "resonance_tester": {"accel_chip": "adxl345"},
    })
}

struct Rig {
    fake: FakeMoonraker,
    printer: FakePrinter,
    moonraker: Moonraker,
}

async fn rig() -> Rig {
    let fake = FakeMoonraker::start().await;
    let printer = FakePrinter::new(&fake, "ready", two_chips());
    let settings = Settings {
        moonraker_url: fake.http_url().parse().unwrap(),
        ..Settings::default()
    };
    let (moonraker, _) = spawn(&settings, fast(), false);
    eventually("current chips", DEADLINE, || {
        moonraker.chips().current && moonraker.control().is_some()
    })
    .await;
    Rig {
        fake,
        printer,
        moonraker,
    }
}

/// Pushes 20-sample batches on `sensor` at `rate` Hz until the bridge is dropped.
fn feed(bridge: &FakeBridge, sensor: &'static str, rate: f64) {
    let bridge = bridge.clone();
    tokio::spawn(async move {
        let mut t = 1000.0;
        loop {
            let rows: Vec<[f64; 4]> = (0..20)
                .map(|i| [t + f64::from(i) / rate, 0.0, 0.0, 9800.0])
                .collect();
            t += 20.0 / rate;
            bridge.push(sensor, &rows, Some((0, 0)));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
}

#[test]
fn a_rate_needs_finite_strictly_increasing_timestamps() {
    let rate = rate_from_times(&[0.0, 0.01, 0.02, 0.03]).unwrap();
    assert!((rate - 100.0).abs() < 1e-6);
    assert_eq!(rate_from_times(&[0.0, 0.01, 0.01]), None);
    assert_eq!(rate_from_times(&[0.0, f64::NAN, 0.02]), None);
    assert_eq!(rate_from_times(&[1.0]), None);
}

#[tokio::test]
async fn two_chips_that_deliver_are_available_with_their_observed_rates() {
    let rig = rig().await;
    feed(rig.fake.bridge(), "adxl345", 3200.0);
    feed(rig.fake.bridge(), "ebb36", 400.0);
    let chips = rig.moonraker.chips().list.chips;
    let outcomes = open_chips(&rig.moonraker, &chips, Duration::from_secs(2))
        .await
        .unwrap();
    let rates: Vec<f64> = outcomes
        .iter()
        .map(|(_, o)| o.as_ref().map(|c| c.observed_rate_hz).unwrap())
        .collect();
    assert!((rates[0] - 3200.0).abs() < 32.0, "{rates:?}");
    assert!((rates[1] - 400.0).abs() < 4.0, "{rates:?}");
    assert_eq!(rig.fake.bridge().open_total(), 2);
    drop(outcomes);
    eventually("the connections to close", DEADLINE, || {
        rig.fake.bridge().open_total() == 0
    })
    .await;
}

#[tokio::test]
async fn a_failed_subscribe_is_unavailable_with_klippers_message_and_the_other_chip_is_fine() {
    let rig = rig().await;
    feed(rig.fake.bridge(), "adxl345", 3200.0);
    rig.fake.bridge().answer_for(
        "ebb36",
        Subscribe::Error("The value 'ebb36' is not valid for sensor".to_owned()),
    );
    let chips = rig.moonraker.chips().list.chips;
    let outcomes = open_chips(&rig.moonraker, &chips, Duration::from_secs(2))
        .await
        .unwrap();
    assert!(outcomes[0].1.is_ok());
    assert_eq!(
        outcomes[1].1.as_ref().err().unwrap(),
        "The value 'ebb36' is not valid for sensor"
    );
}

#[tokio::test]
async fn a_chip_with_no_data_is_unavailable_after_the_timeout_and_chips_wait_in_parallel() {
    let rig = rig().await;
    // Both subscribe fine and neither ever delivers.
    let chips = rig.moonraker.chips().list.chips;
    let started = Instant::now();
    let outcomes = open_chips(&rig.moonraker, &chips, Duration::from_millis(400))
        .await
        .unwrap();
    let took = started.elapsed();
    for (_, outcome) in &outcomes {
        assert!(outcome.as_ref().err().unwrap().contains("no valid samples"));
    }
    assert!(
        took < Duration::from_millis(700),
        "checked in series: {took:?}"
    );
}

#[tokio::test]
async fn samples_with_a_repeated_timestamp_are_not_valid() {
    let rig = rig().await;
    let bridge = rig.fake.bridge().clone();
    tokio::spawn(async move {
        loop {
            bridge.push(
                "adxl345",
                &[[5.0, 0.0, 0.0, 0.0], [5.0, 0.0, 0.0, 0.0]],
                None,
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
    let chips = rig.moonraker.chips().list.chips[..1].to_vec();
    let outcomes = open_chips(&rig.moonraker, &chips, Duration::from_secs(1))
        .await
        .unwrap();
    assert!(outcomes[0].1.as_ref().err().unwrap().contains("not valid"));
}

#[tokio::test]
async fn the_check_endpoint_records_availability_and_a_restart_forgets_it() {
    let rig = rig().await;
    feed(rig.fake.bridge(), "adxl345", 3200.0);
    rig.fake
        .bridge()
        .answer_for("ebb36", Subscribe::Error("nope".to_owned()));
    let app = mount(cell(None), rig.moonraker.clone());
    let (status, body) = json_post(&app, "/api/chips/check").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["chips"][0]["availability"]["state"], "available");
    assert_eq!(body["chips"][1]["availability"]["state"], "unavailable");
    assert_eq!(body["chips"][1]["availability"]["reason"], "nope");
    assert!(matches!(
        rig.moonraker.chips().list.chips[0].availability,
        Availability::Available { .. }
    ));
    eventually("the check's connections to close", DEADLINE, || {
        rig.fake.bridge().open_total() == 0
    })
    .await;

    rig.printer.set_state("disconnected");
    rig.fake.push("notify_klippy_disconnected", json!([]));
    eventually("chips not current", DEADLINE, || {
        !rig.moonraker.chips().current
    })
    .await;
    assert!(
        rig.moonraker
            .chips()
            .list
            .chips
            .iter()
            .all(|c| c.availability == Availability::Unchecked)
    );
}

#[tokio::test]
async fn a_check_while_klipper_is_shut_down_is_refused_and_opens_no_bridge() {
    let rig = rig().await;
    rig.printer.set_state("shutdown");
    rig.fake.push("notify_klippy_shutdown", json!([]));
    eventually("shutdown", DEADLINE, || {
        rig.moonraker.status().klippy.state == KlippyState::Shutdown
    })
    .await;
    let app = mount(cell(None), rig.moonraker.clone());
    let (status, body) = json_post(&app, "/api/chips/check").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "preflight_failed");
    assert_eq!(body["failures"][0]["check"], "klipper_ready");
    assert_eq!(rig.fake.bridge().subscribes(), 0);
}
