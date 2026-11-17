mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::fake_moonraker::{Gate, Script};
use common::sim_printer::{Rig, rig};
use serde_json::{Value, json};
use wobblescope::capture::live::{FinishedOutcome, FinishedTest};
use wobblescope::capture::request::TestPlan;
use wobblescope::capture::vibrations::{VibrationsRequest, plan_vibrations};
use wobblescope::moonraker::settings::parse_settings;
use wobblescope_session::meta::{Outcome, SessionKind, SessionMeta};

const CHIPS: [&str; 2] = ["adxl345", "lis2dw ebb36"];

/// A plan with two speeds (2 and 10 mm/s) per angle, so a run takes four steps.
fn plan() -> TestPlan {
    let mut printer = parse_settings(
        &common::fake_printer::fixture("real_settings.json"),
        &common::fake_printer::fixture("real_printer_info.json"),
    )
    .unwrap();
    // The fixture's config has no `[input_shaper]` section, which the profile needs.
    printer.input_shaper_configured = true;
    let request = VibrationsRequest {
        chips: CHIPS.iter().map(|c| (*c).to_owned()).collect(),
        size: Some(50.0),
        z_height: None,
        max_speed: Some(10.0),
        speed_increment: Some(8.0),
        accel: Some(3000.0),
        travel_speed: Some(120.0),
    };
    plan_vibrations(&request, &printer, 600, &[Some(500.0), Some(400.0)]).unwrap()
}

/// A rig whose toolhead reports the limits the run saves and whose config has an `[input_shaper]`
/// section. Klipper's `input_shaper` status object holds values only when `shaper` is true.
async fn vibrations_rig(shaper: bool) -> Rig {
    let rig = rig().await;
    let printer = &rig.sim.printer;
    let mut settings = common::fake_printer::fixture("real_settings.json")["status"]["configfile"]
        ["settings"]
        .clone();
    settings["input_shaper"] = json!({"shaper_type_x": "mzv", "shaper_freq_x": "51.2"});
    printer.set_settings(settings);
    printer.set_field("toolhead", "max_velocity", json!(500.0));
    printer.set_field("toolhead", "max_accel", json!(3000.0));
    printer.set_field("toolhead", "minimum_cruise_ratio", json!(0.5));
    printer.set_field("toolhead", "square_corner_velocity", json!(8.0));
    if shaper {
        for (field, value) in [
            ("shaper_type_x", json!("mzv")),
            ("shaper_freq_x", json!(51.2)),
            ("damping_ratio_x", json!(0.1)),
            ("shaper_type_y", json!("mzv")),
            ("shaper_freq_y", json!(44.0)),
            ("damping_ratio_y", json!(0.1)),
        ] {
            printer.set_field("input_shaper", field, value);
        }
    }
    rig
}

async fn run(rig: &Rig, plan: &TestPlan) -> FinishedTest {
    let task = match rig.start(plan).await {
        Ok(task) => task,
        Err(_) => panic!("pre-flight must pass"),
    };
    let finished = rig.finished().await;
    task.await.unwrap();
    finished
}

fn meta_of(rig: &Rig, finished: &FinishedTest) -> SessionMeta {
    rig.storage
        .session(finished.session_id.as_ref().expect("a session"))
        .unwrap()
        .1
}

fn is_step(script: &str) -> bool {
    script.contains("G4 P300")
}

fn is_restore(script: &str) -> bool {
    script.contains("RESTORE_GCODE_STATE NAME=wobblescope_calibration")
}

fn reason(f: &FinishedTest) -> String {
    f.reason.clone().unwrap_or_default()
}

/// Calls `act` when the nth step script (from 0) arrives, and lets the script run.
fn on_step(rig: &Rig, nth: usize, act: impl Fn() + Send + Sync + 'static) {
    let seen = std::sync::atomic::AtomicUsize::new(0);
    rig.sim.intercept(move |script| {
        if is_step(script) && seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == nth {
            act();
        }
        None
    });
}

fn refuse_script(rig: &Rig, matches: impl Fn(&str) -> bool + Send + Sync + 'static) {
    let done = std::sync::atomic::AtomicBool::new(false);
    rig.sim.intercept(move |script| {
        (matches(script) && !done.swap(true, std::sync::atomic::Ordering::SeqCst))
            .then(|| Script::error(400, "Move out of range"))
    });
}

fn assert_clean(rig: &Rig) {
    assert!(!rig.slot.is_held(), "the slot is still held");
    assert_eq!(rig.storage.usage().unwrap().reserved_bytes, 0);
    assert_eq!(rig.bridges_open(), 0, "a bridge connection is still open");
}

fn assert_restored(rig: &Rig) {
    let scripts = rig.sim.scripts();
    let restore = scripts.last().unwrap();
    assert!(is_restore(restore), "{scripts:#?}");
    assert!(
        restore.contains("SET_VELOCITY_LIMIT VELOCITY=500 ACCEL=3000 MINIMUM_CRUISE_RATIO=0.5 SQUARE_CORNER_VELOCITY=8"),
        "{restore}"
    );
    assert!(!restore.contains("SET_INPUT_SHAPER"), "{restore}");
}

#[tokio::test]
async fn a_full_run_stores_two_segments_with_steps_in_order() {
    let rig = vibrations_rig(true).await;
    let plan = plan();
    let finished = run(&rig, &plan).await;
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    assert!(!finished.needs_restart);
    let meta = meta_of(&rig, &finished);
    assert_eq!(meta.kind(), SessionKind::Vibrations);
    assert_eq!(meta.outcome, Outcome::Complete);
    assert!(meta.vibrations_run.is_some());
    assert_eq!(meta.segments.len(), 2);
    let vibrations = plan.vibrations.as_ref().unwrap();
    for (segment, angle) in meta.segments.iter().zip(vibrations.angles) {
        assert_eq!(segment.axis, plan.segments[usize::from(angle > 100.0)].axis);
        let speeds: Vec<f64> = segment.steps.iter().map(|s| s.speed).collect();
        assert_eq!(speeds, vibrations.speeds);
    }
    let scripts = rig.sim.scripts();
    assert_eq!(scripts.iter().filter(|s| is_step(s)).count(), 4);
    assert_restored(&rig);
    assert_clean(&rig);
}

#[tokio::test]
async fn each_chip_has_one_file_per_angle() {
    let rig = vibrations_rig(true).await;
    let finished = run(&rig, &plan()).await;
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    let meta = meta_of(&rig, &finished);
    for segment in &meta.segments {
        let chips: Vec<u32> = segment.files.iter().map(|f| f.chip).collect();
        assert_eq!(chips, [0, 1]);
    }
}

#[tokio::test]
async fn step_windows_do_not_overlap_and_lie_inside_the_segment_window() {
    let rig = vibrations_rig(true).await;
    let finished = run(&rig, &plan()).await;
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    let meta = meta_of(&rig, &finished);
    for segment in &meta.segments {
        let outer = segment.window.unwrap();
        let mut previous_end = f64::NEG_INFINITY;
        for step in &segment.steps {
            let w = step.window;
            assert!(w.start < w.end);
            assert!(w.start >= previous_end);
            assert!(w.start >= outer.start && w.end <= outer.end);
            previous_end = w.end;
        }
        assert_eq!(segment.steps[0].window.start, outer.start);
        assert_eq!(segment.steps.last().unwrap().window.end, outer.end);
    }
    assert!(meta.segments[0].window.unwrap().end <= meta.segments[1].window.unwrap().start);
}

#[tokio::test]
async fn a_stop_in_the_second_angle_keeps_the_first_and_the_finished_steps_of_the_second() {
    let rig = vibrations_rig(true).await;
    let slot = rig.slot.clone();
    // The first step of the second angle runs. The stop is seen before the next one.
    on_step(&rig, 2, move || {
        assert!(slot.request_stop_current().is_some());
    });
    let finished = run(&rig, &plan()).await;
    assert_eq!(finished.outcome, FinishedOutcome::Failed, "{finished:?}");
    assert_eq!(reason(&finished), "emergency stop");
    assert!(!finished.needs_restart);
    let meta = meta_of(&rig, &finished);
    assert_eq!(meta.segments.len(), 2);
    assert_eq!(meta.segments[0].steps.len(), 2);
    assert_eq!(meta.segments[1].steps.len(), 1);
    assert!(matches!(meta.outcome, Outcome::Failed { .. }));
    meta.validate().unwrap();
    let second = &meta.segments[1];
    assert_eq!(second.window.unwrap().end, second.steps[0].window.end);
    assert_eq!(second.files.len(), 2);
    assert_restored(&rig);
    assert_clean(&rig);
}

#[tokio::test]
async fn a_stop_in_the_middle_of_a_step_writes_no_samples_past_the_last_finished_step() {
    let rig = vibrations_rig(true).await;
    let slot = rig.slot.clone();
    // The second step of the second angle is the fourth step script. It never finishes: the chips
    // keep delivering for it and the stop arrives while it runs.
    let sim = rig.sim.clone();
    let seen = std::sync::atomic::AtomicUsize::new(0);
    rig.sim.intercept(move |script| {
        if is_step(script) && seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 3 {
            sim.stream(2.0);
            assert!(slot.request_stop_current().is_some());
            return Some(Script::Silent);
        }
        None
    });
    let finished = run(&rig, &plan()).await;
    assert_eq!(finished.outcome, FinishedOutcome::Failed, "{finished:?}");
    let meta = meta_of(&rig, &finished);
    assert_eq!(meta.segments.len(), 2);
    let interrupted = &meta.segments[1];
    assert_eq!(interrupted.steps.len(), 1);
    let window = interrupted.window.unwrap();
    assert_eq!(window.end, interrupted.steps[0].window.end);
    assert_eq!(interrupted.files.len(), 2);
    let id = finished.session_id.as_ref().unwrap();
    for file in &interrupted.files {
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(
            &mut rig.storage.open_sample(id, file.chip, 1).unwrap(),
            &mut bytes,
        )
        .unwrap();
        let samples = wobblescope_session::codec::decode_samples(&bytes).unwrap();
        assert_eq!(samples.len(), file.sample_count as usize);
        assert!(!samples.t.is_empty());
        let first = samples.t.iter().copied().fold(f64::INFINITY, f64::min);
        let last = samples.t.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(first >= window.start, "{first} < {}", window.start);
        assert!(last < window.end, "{last} >= {}", window.end);
    }
    assert_restored(&rig);
    assert_clean(&rig);
}

#[tokio::test]
async fn a_stop_before_any_step_finishes_leaves_no_session() {
    let rig = vibrations_rig(true).await;
    let slot = rig.slot.clone();
    // The stop comes with the move to Z, so no step ever starts.
    rig.sim.intercept(move |script| {
        if script.contains("G1 Z") {
            assert!(slot.request_stop_current().is_some());
        }
        None
    });
    let finished = run(&rig, &plan()).await;
    assert_eq!(finished.outcome, FinishedOutcome::NoSession, "{finished:?}");
    assert_eq!(reason(&finished), "emergency stop");
    assert!(rig.sessions().is_empty());
    assert!(!rig.sim.scripts().iter().any(|s| is_step(s)));
    assert_restored(&rig);
    assert_clean(&rig);
}

#[tokio::test]
async fn acceleration_limits_are_restored_after_failure() {
    let rig = vibrations_rig(true).await;
    let seen = std::sync::atomic::AtomicUsize::new(0);
    rig.sim.intercept(move |script| {
        (is_step(script) && seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1)
            .then(|| Script::error(400, "Move out of range"))
    });
    let finished = run(&rig, &plan()).await;
    assert_eq!(finished.outcome, FinishedOutcome::Failed, "{finished:?}");
    assert!(reason(&finished).contains("Move out of range"));
    assert!(!finished.needs_restart);
    let meta = meta_of(&rig, &finished);
    assert_eq!(meta.segments.len(), 1);
    assert_eq!(meta.segments[0].steps.len(), 1);
    assert_restored(&rig);
    assert_clean(&rig);
}

#[tokio::test]
async fn the_run_positions_z_then_xy_then_dwells_before_the_first_step() {
    let rig = vibrations_rig(true).await;
    let finished = run(&rig, &plan()).await;
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    let scripts = rig.sim.scripts();
    assert_eq!(scripts[0], "SAVE_GCODE_STATE NAME=wobblescope_calibration");
    assert_eq!(
        scripts[1],
        "SET_VELOCITY_LIMIT ACCEL=3000 MINIMUM_CRUISE_RATIO=0 SQUARE_CORNER_VELOCITY=5.0"
    );
    // Z at a tenth of the travel speed (12 mm/s), then the middle of the travel minus 15 mm
    // (X 0 to 301 and Y 0 to 308) at the travel speed (120 mm/s), then the dwell.
    assert!(scripts[2].contains("G1 Z20 F720"), "{}", scripts[2]);
    assert!(
        scripts[3].contains("G1 X135.5 Y139 F7200"),
        "{}",
        scripts[3]
    );
    assert!(scripts[4].contains("G4 P500"), "{}", scripts[4]);
    assert!(is_step(&scripts[5]), "{}", scripts[5]);
}

#[tokio::test]
async fn a_failure_during_positioning_restores_the_limits() {
    let rig = vibrations_rig(true).await;
    refuse_script(&rig, |script| script.contains("G1 X135.5"));
    let finished = run(&rig, &plan()).await;
    assert_eq!(finished.outcome, FinishedOutcome::NoSession, "{finished:?}");
    assert!(reason(&finished).contains("Move out of range"));
    assert!(!finished.needs_restart);
    assert!(rig.sessions().is_empty());
    assert!(!rig.sim.scripts().iter().any(|s| is_step(s)));
    assert_restored(&rig);
    assert_clean(&rig);
}

#[tokio::test]
async fn an_empty_input_shaper_status_does_not_stop_a_run() {
    // Some Klipper versions report an empty `input_shaper` object on a configured printer. The
    // run is refused in pre-flight from the config section, never from this status.
    let rig = vibrations_rig(false).await;
    let finished = run(&rig, &plan()).await;
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    assert_restored(&rig);
    assert_clean(&rig);
}

fn post(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("x-wobblescope-client", "1")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn the_run_refuses_while_another_test_holds_the_slot() {
    let rig = vibrations_rig(true).await;
    let held = rig.slot.try_acquire().expect("the slot is free");
    let body = json!({"chips": CHIPS, "size": 50, "max_speed": 10, "speed_increment": 8});
    let (status, _, bytes) = common::send(&rig.app(), post("/api/vibrations", &body)).await;
    let answer: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert!(
        answer["failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["check"] == "no_test_running"),
        "{answer}"
    );
    assert!(rig.sim.scripts().is_empty());
    drop(held);
}

#[tokio::test]
async fn posting_a_run_starts_it_and_returns_202() {
    let rig = vibrations_rig(true).await;
    rig.sim.feed_until_motion();
    let body = json!({"chips": CHIPS, "size": 50, "max_speed": 10, "speed_increment": 8});
    let (status, _, bytes) = common::send(&rig.app(), post("/api/vibrations", &body)).await;
    let answer: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(status, StatusCode::ACCEPTED, "{answer}");
    let finished = rig.finished().await;
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    assert_eq!(finished.session_id.as_deref(), answer["id"].as_str());
}

#[tokio::test]
async fn a_chip_that_dies_mid_run_restores_the_limits_and_keeps_finished_steps() {
    let rig = vibrations_rig(true).await;
    let sim = rig.sim.clone();
    on_step(&rig, 2, move || sim.bridge().drop_sensor("ebb36"));
    let finished = run(&rig, &plan()).await;
    assert_ne!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    assert!(!finished.needs_restart);
    let meta = meta_of(&rig, &finished);
    assert!(
        matches!(meta.outcome, Outcome::Failed { .. }),
        "{:?}",
        meta.outcome
    );
    assert_eq!(meta.segments[0].steps.len(), 2);
    assert!(meta.segments.iter().map(|s| s.steps.len()).sum::<usize>() >= 2);
    meta.validate().unwrap();
    assert_restored(&rig);
    assert_clean(&rig);
}

#[tokio::test]
async fn klipper_going_down_mid_run_ends_the_session_without_a_restore_attempt_that_hangs() {
    let rig = vibrations_rig(true).await;
    let gate = Gate::new();
    let held = gate.clone();
    let seen = std::sync::atomic::AtomicUsize::new(0);
    rig.sim.intercept(move |script| {
        (is_step(script) && seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 2)
            .then(|| Script::Gated(held.clone()))
    });
    let task = match rig.start(&plan()).await {
        Ok(task) => task,
        Err(_) => panic!("pre-flight must pass"),
    };
    rig.until("the third step", || {
        rig.sim.scripts().iter().filter(|s| is_step(s)).count() == 3
    })
    .await;
    rig.sim.printer.set_state("shutdown");
    rig.sim
        .printer
        .set_message("MCU 'mcu' shutdown: Timer too close");
    rig.sim.fake.push("notify_klippy_shutdown", json!([]));
    rig.until("Klipper down", || {
        rig.moonraker.status().klippy.state != wobblescope::moonraker::status::KlippyState::Ready
    })
    .await;
    gate.release(Script::error(400, "Klipper state: Shutdown"));
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed, "{finished:?}");
    assert!(
        reason(&finished).contains("Timer too close"),
        "{}",
        reason(&finished)
    );
    assert!(!finished.needs_restart);
    let scripts = rig.sim.scripts();
    assert!(!scripts.iter().any(|s| is_restore(s)), "{scripts:#?}");
    let meta = meta_of(&rig, &finished);
    assert_eq!(meta.segments[0].steps.len(), 2);
    assert_clean(&rig);
}
