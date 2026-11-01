mod common;

use std::time::Duration;

use common::fake_bridge::{FakeBridge, Subscribe};
use common::fake_moonraker::{FakeMoonraker, eventually};
use common::fake_printer::FakePrinter;
use common::{cell, open, test_plan};
use serde_json::{Value, json};
use tempfile::TempDir;
use time::OffsetDateTime;
use wobblescope::capture::preflight::{Deps, Refusal, Started, run};
use wobblescope::capture::slot::TestSlot;
use wobblescope::config::Settings;
use wobblescope::moonraker::status::KlippyState;
use wobblescope::moonraker::{Moonraker, Timings, spawn};
use wobblescope::startup::StorageCell;
use wobblescope::storage::{Storage, StorageError};
use wobblescope_core::fit::Axis;

const DEADLINE: Duration = Duration::from_secs(10);

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
    storage: Storage,
    cell: StorageCell,
    slot: TestSlot,
    _dir: TempDir,
}

async fn rig_with(cap: u64) -> Rig {
    let fake = FakeMoonraker::start().await;
    let printer = FakePrinter::new(&fake, "ready", two_chips());
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
    eventually("current chips", DEADLINE, || {
        moonraker.chips().current && moonraker.control().is_some()
    })
    .await;
    let dir = TempDir::new().unwrap();
    let (storage, _) = open(dir.path(), cap);
    Rig {
        fake,
        printer,
        moonraker,
        cell: cell(Some(storage.clone())),
        storage,
        slot: TestSlot::default(),
        _dir: dir,
    }
}

async fn rig() -> Rig {
    rig_with(u64::MAX / 4).await
}

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

async fn attempt(rig: &Rig, busy: bool) -> Result<Started, Refusal> {
    let plan = test_plan(&["adxl345", "lis2dw ebb36"], &[Axis::X]);
    let mut deps = Deps::new(&rig.moonraker, &rig.cell, &rig.slot);
    deps.availability_timeout = Duration::from_millis(500);
    run(&deps, &plan, busy, OffsetDateTime::now_utc()).await
}

fn failed_checks(result: Result<Started, Refusal>) -> Vec<String> {
    match result {
        Err(Refusal::Checks(failures)) => failures.into_iter().map(|f| f.check).collect(),
        Err(Refusal::Storage(e)) => panic!("a storage refusal: {e}"),
        Ok(_) => panic!("the test started"),
    }
}

#[tokio::test]
async fn everything_good_starts_with_open_recorders_and_a_reservation_and_sends_no_motion() {
    let rig = rig().await;
    feed(rig.fake.bridge(), "adxl345", 3200.0);
    feed(rig.fake.bridge(), "ebb36", 400.0);
    let Ok(started) = attempt(&rig, false).await else {
        panic!("pre-flight must pass");
    };
    assert_eq!(started.chips.len(), 2);
    assert!(started.id.starts_with("test-"));
    assert_eq!(rig.fake.bridge().open_total(), 2);
    assert!(rig.storage.usage().unwrap().reserved_bytes > 0);
    assert!(rig.fake.requests_for("printer.gcode.script").is_empty());
}

#[tokio::test]
async fn a_shutdown_a_print_and_an_unhomed_z_give_three_failures_and_open_no_bridge() {
    let rig = rig().await;
    rig.printer.set_state("shutdown");
    rig.fake.push("notify_klippy_shutdown", json!([]));
    rig.printer
        .set_field("print_stats", "state", json!("printing"));
    rig.printer.set_field("toolhead", "homed_axes", json!("xy"));
    eventually("shutdown", DEADLINE, || {
        rig.moonraker.status().klippy.state == KlippyState::Shutdown
    })
    .await;
    let failed = failed_checks(attempt(&rig, false).await);
    assert_eq!(failed, ["klipper_ready", "not_printing", "homed"]);
    assert_eq!(rig.fake.bridge().subscribes(), 0);
    assert!(rig.fake.requests_for("printer.gcode.script").is_empty());
}

#[tokio::test]
async fn a_second_request_while_the_slot_is_held_fails_no_test_running() {
    let rig = rig().await;
    let permit = rig.slot.try_acquire().unwrap();
    assert!(rig.slot.try_acquire().is_none());
    let failed = failed_checks(attempt(&rig, true).await);
    assert_eq!(failed, ["no_test_running"]);
    drop(permit);
    assert!(rig.slot.try_acquire().is_some());
}

#[tokio::test]
async fn a_toolhead_that_is_printing_fails_toolhead_idle() {
    let rig = rig().await;
    rig.printer
        .set_field("idle_timeout", "state", json!("Printing"));
    assert_eq!(failed_checks(attempt(&rig, false).await), ["toolhead_idle"]);
}

#[tokio::test]
async fn no_idle_state_passes_normally_and_fails_during_quarantine() {
    let rig = rig().await;
    feed(rig.fake.bridge(), "adxl345", 3200.0);
    feed(rig.fake.bridge(), "ebb36", 400.0);
    rig.printer.remove_object("idle_timeout");
    assert!(attempt(&rig, false).await.is_ok());

    let rig = self::rig().await;
    rig.printer.remove_object("idle_timeout");
    rig.slot.start_quarantine(Duration::from_secs(60));
    let failed = failed_checks(attempt(&rig, false).await);
    assert_eq!(failed, ["toolhead_idle", "quarantine"]);
}

#[tokio::test]
async fn the_quarantine_refuses_with_the_time_left_and_lets_go_when_it_ends() {
    let rig = rig().await;
    feed(rig.fake.bridge(), "adxl345", 3200.0);
    feed(rig.fake.bridge(), "ebb36", 400.0);
    rig.slot.start_quarantine(Duration::from_millis(400));
    match attempt(&rig, false).await {
        Err(Refusal::Checks(failures)) => {
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].check, "quarantine");
            assert!(
                failures[0].message.contains("another"),
                "{}",
                failures[0].message
            );
        }
        _ => panic!("expected a quarantine refusal"),
    }
    tokio::time::sleep(Duration::from_millis(450)).await;
    assert!(attempt(&rig, false).await.is_ok());
}

#[tokio::test]
async fn one_chip_unavailable_refuses_naming_it_and_closes_the_other_connection() {
    let rig = rig().await;
    feed(rig.fake.bridge(), "adxl345", 3200.0);
    rig.fake
        .bridge()
        .answer_for("ebb36", Subscribe::Error("not valid".to_owned()));
    match attempt(&rig, false).await {
        Err(Refusal::Checks(failures)) => {
            assert_eq!(failures.len(), 1);
            assert_eq!(failures[0].check, "chips_available");
            assert!(failures[0].message.contains("lis2dw ebb36"));
            assert!(failures[0].message.contains("not valid"));
        }
        _ => panic!("expected a chips refusal"),
    }
    eventually("the other chip's connection to close", DEADLINE, || {
        rig.fake.bridge().open_total() == 0
    })
    .await;
    assert_eq!(rig.storage.usage().unwrap().reserved_bytes, 0);
}

#[tokio::test]
async fn an_unknown_chip_is_refused_without_opening_anything() {
    let rig = rig().await;
    let plan = test_plan(&["adxl345", "nope"], &[Axis::X]);
    let deps = Deps::new(&rig.moonraker, &rig.cell, &rig.slot);
    let Err(Refusal::Checks(failures)) = run(&deps, &plan, false, OffsetDateTime::now_utc()).await
    else {
        panic!("expected a refusal");
    };
    assert!(failures[0].message.contains("nope"));
    assert_eq!(rig.fake.bridge().subscribes(), 0);
}

#[tokio::test]
async fn an_estimate_over_the_cap_is_a_storage_refusal_not_a_failed_check() {
    let rig = rig_with(1_000).await;
    feed(rig.fake.bridge(), "adxl345", 3200.0);
    feed(rig.fake.bridge(), "ebb36", 400.0);
    match attempt(&rig, false).await {
        Err(Refusal::Storage(StorageError::CapExceeded { .. })) => {}
        other => panic!("expected the cap refusal, got {:?}", other.is_ok()),
    }
    assert_eq!(rig.storage.usage().unwrap().reserved_bytes, 0);
}

#[tokio::test]
async fn refused_then_accepted_leaves_nothing_held_by_the_refusals() {
    let rig = rig().await;
    feed(rig.fake.bridge(), "adxl345", 3200.0);
    feed(rig.fake.bridge(), "ebb36", 400.0);
    rig.printer.set_field("toolhead", "homed_axes", json!(""));
    assert_eq!(failed_checks(attempt(&rig, false).await), ["homed"]);
    assert_eq!(rig.storage.usage().unwrap().reserved_bytes, 0);
    assert!(!rig.slot.is_held());
    rig.printer
        .set_field("toolhead", "homed_axes", json!("xyz"));
    let Ok(started) = attempt(&rig, false).await else {
        panic!("pre-flight must pass");
    };
    assert!(rig.storage.usage().unwrap().reserved_bytes > 0);
    drop(started);
    assert_eq!(rig.storage.usage().unwrap().reserved_bytes, 0);
}
