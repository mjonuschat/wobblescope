mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::fake_moonraker::{Gate, Script};
use common::sim_printer::{Move, Rig, Sweep, When, fast_runner, rig, rig_with};
use common::{open, test_plan};
use serde_json::json;
use wobblescope::capture::axes_map::{AxesMapRequest, ChipChoice, plan_axes_map};
use wobblescope::capture::live::{FinishedOutcome, FinishedTest, Phase, RunKind};
use wobblescope::capture::preflight::Refusal;
use wobblescope::capture::request::{Overrides, TestPlan, TestRequest, plan};
use wobblescope::moonraker::settings::parse_settings;
use wobblescope_core::axes_map::Mounting;
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::fit::Axis;
use wobblescope_session::meta::{Mode, Outcome, SessionKind};

const CHIPS: [&str; 2] = ["adxl345", "lis2dw ebb36"];

#[tokio::test]
async fn two_chips_two_axes_one_point_finish_as_a_complete_session() {
    let rig = rig().await;
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let Ok(task) = rig.start(&plan).await else {
        panic!("pre-flight must pass");
    };
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    let id = finished.session_id.clone().unwrap();
    assert_eq!(rig.sessions(), std::slice::from_ref(&id));

    let (_, meta) = rig.storage.session(&id).unwrap();
    assert_eq!(meta.outcome, Outcome::Complete);
    assert_eq!(meta.segments.len(), 2);
    assert_eq!(meta.chips.len(), 2);
    assert_eq!(meta.segments[0].files.len(), 2);
    assert_eq!(meta.segments[1].files.len(), 2);
    for segment in &meta.segments {
        let window = segment.window.unwrap();
        assert!(window.end - window.start > 1.2 && window.end - window.start < 1.4);
        assert!(
            segment
                .files
                .iter()
                .all(|f| f.coverage == Coverage::Complete)
        );
    }
    // Windows follow the simulated print time: 0.5 s of moving, the lead-in, then the sweep.
    let w0 = meta.segments[0].window.unwrap();
    assert!((w0.start - 101.2).abs() < 1e-6, "{w0:?}");
    assert!(meta.chips.iter().all(|c| c.health == Health::Ok));
    // The simulated chips deliver 500 and 400 Hz, under half of the 3200 and 1600 Hz nominal.
    for chip in &meta.chips {
        assert_eq!(chip.health_notes.len(), 1, "{:?}", chip.health_notes);
        assert!(
            chip.health_notes[0].contains("under half"),
            "{:?}",
            chip.health_notes
        );
    }
    // The chips' wiring: the configured axes map, and the firmware of the MCUs they sit on.
    assert_eq!(meta.chips[0].axes_map, None);
    assert_eq!(meta.chips[1].axes_map.as_deref(), Some("x,z,-y"));
    let printer = meta.printer.as_ref().unwrap();
    assert_eq!(
        printer.mcu_firmware,
        ["ebb36: v0.13.0-820-g0cee841e", "mcu: v0.13.0-658-gbd09e017"]
    );
    assert_eq!(meta.probe_points.len(), 1);
    assert_eq!(rig.sim.sweeps().len(), 2);
    assert!(rig.sim.sweeps()[0].contains("AXIS=X") && rig.sim.sweeps()[1].contains("AXIS=Y"));

    // Everything is cleaned up.
    assert_eq!(rig.bridges_open(), 0);
    assert_eq!(rig.storage.usage().unwrap().reserved_bytes, 0);
    assert_eq!(rig.leftovers(), [id]);
    assert!(!rig.slot.is_held());
}

#[tokio::test]
async fn the_session_records_the_printers_corner_velocity_and_damping_ratios() {
    let rig = rig().await;
    let mut settings = common::fake_printer::fixture("real_settings.json")["status"]["configfile"]
        ["settings"]
        .clone();
    settings["input_shaper"] = json!({"damping_ratio_x": 0.12});
    rig.sim.printer.set_settings(settings);
    rig.sim
        .printer
        .set_field("toolhead", "square_corner_velocity", json!(8.0));
    let plan = test_plan(&CHIPS, &[Axis::X]);
    let task = start(&rig, &plan).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    let id = finished.session_id.unwrap();
    let (_, meta) = rig.storage.session(&id).unwrap();
    let printer = meta.printer.unwrap();
    assert_eq!(printer.square_corner_velocity, Some(8.0));
    assert_eq!(printer.damping_ratio_x, Some(0.12));
    assert_eq!(printer.damping_ratio_y, None);
}

// ---- helpers ----

/// Starts `plan` and returns its task.
async fn start(
    rig: &Rig,
    plan: &wobblescope::capture::request::TestPlan,
) -> tokio::task::JoinHandle<()> {
    match rig.start(plan).await {
        Ok(task) => task,
        Err(Refusal::Checks(failures)) => panic!("pre-flight refused: {failures:?}"),
        Err(Refusal::Storage(e)) => panic!("pre-flight refused: {e}"),
    }
}

/// The running test's id, once it has one.
async fn running_id(rig: &Rig) -> String {
    rig.until("a running test", || rig.snapshot().test.is_some())
        .await;
    rig.snapshot().test.unwrap().id
}

/// After any ending: the slot, the reservation and every bridge are released.
fn assert_clean(rig: &Rig) {
    assert!(!rig.slot.is_held(), "the slot is still held");
    assert_eq!(rig.storage.usage().unwrap().reserved_bytes, 0);
    assert_eq!(rig.bridges_open(), 0, "a bridge connection is still open");
}

fn reason(f: &FinishedTest) -> String {
    f.reason.clone().unwrap_or_default()
}

fn plan_with_points(points: &[[f64; 3]], axes: &[Axis]) -> wobblescope::capture::request::TestPlan {
    let printer = parse_settings(
        &common::fake_printer::fixture("real_settings.json"),
        &common::fake_printer::fixture("real_printer_info.json"),
    )
    .unwrap();
    let request = TestRequest {
        chips: CHIPS.iter().map(|c| (*c).to_owned()).collect(),
        axes: axes.to_vec(),
        mode: Mode::Pulse,
        overrides: Overrides::default(),
        points: Some(points.to_vec()),
    };
    plan(&request, &printer, 600).unwrap()
}

// ---- happy paths ----

#[tokio::test]
async fn four_segments_over_two_points_run_in_order_each_after_its_positioning() {
    let rig = rig().await;
    let plan = plan_with_points(
        &[[100.0, 100.0, 20.0], [200.0, 150.0, 20.0]],
        &[Axis::X, Axis::Y],
    );
    let task = start(&rig, &plan).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    let kinds: Vec<String> = rig
        .sim
        .scripts()
        .iter()
        .map(|s| {
            if s.starts_with("TEST_RESONANCES") {
                let axis = if s.contains("AXIS=X") { "X" } else { "Y" };
                let at = if s.contains("POINT=100,") { "p0" } else { "p1" };
                format!("sweep {at}{axis}")
            } else {
                "move".to_owned()
            }
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "move",
            "sweep p0X",
            "move",
            "sweep p0Y",
            "move",
            "sweep p1X",
            "move",
            "sweep p1Y"
        ]
    );
    assert_clean(&rig);
}

#[tokio::test]
async fn a_chip_that_stops_delivering_makes_the_session_incomplete_and_keeps_its_other_data() {
    let rig = rig().await;
    rig.sim.silence_when(Some("ebb36"), When::SweepStart(1));
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let task = start(&rig, &plan).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(
        finished.outcome,
        FinishedOutcome::Incomplete,
        "{finished:?}"
    );
    let (_, meta) = rig.storage.session(&finished.session_id.unwrap()).unwrap();
    assert!(matches!(meta.outcome, Outcome::Incomplete { .. }));
    let ok = |segment: usize, chip: u32| meta.find_file(chip, segment).unwrap().coverage.clone();
    assert_eq!(ok(0, 0), Coverage::Complete);
    assert_eq!(ok(0, 1), Coverage::Complete);
    assert_eq!(ok(1, 0), Coverage::Complete);
    assert!(matches!(ok(1, 1), Coverage::Incomplete { .. }));
    assert_clean(&rig);
}

// ---- stops and failures during a sweep ----

#[tokio::test]
async fn a_stop_during_segment_two_fails_the_test_keeps_segment_one_and_stores_two_incomplete() {
    let rig = rig().await;
    rig.sim.set_behavior(1, Sweep::Pending);
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let task = start(&rig, &plan).await;
    let id = running_id(&rig).await;
    rig.until("the second sweep", || rig.sim.sweeps().len() == 2)
        .await;
    assert!(rig.slot.request_stop(&id));
    // Klipper shuts down, so the pending call fails.
    rig.sim.release_pending(false);
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert_eq!(reason(&finished), "emergency stop");
    assert!(!finished.motion_uncertain);
    let (_, meta) = rig.storage.session(&id).unwrap();
    assert_eq!(
        meta.outcome,
        Outcome::Failed {
            reason: "emergency stop".to_owned()
        }
    );
    assert_eq!(meta.segments.len(), 2);
    assert!(
        meta.segments[0]
            .files
            .iter()
            .all(|f| f.coverage == Coverage::Complete)
    );
    assert!(meta.segments[1].window.is_some());
    assert!(
        meta.segments[1]
            .files
            .iter()
            .all(|f| matches!(f.coverage, Coverage::Incomplete { .. }))
    );
    assert_clean(&rig);
}

#[tokio::test]
async fn a_klipper_shutdown_during_the_sweep_fails_with_klippers_message_and_keeps_the_segment() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    let plan = test_plan(&CHIPS, &[Axis::X]);
    let task = start(&rig, &plan).await;
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;
    rig.sim.printer.set_state("shutdown");
    rig.sim
        .printer
        .set_message("MCU 'mcu' shutdown: Timer too close");
    rig.sim.fake.push("notify_klippy_shutdown", json!([]));
    rig.until("Klipper down", || {
        rig.moonraker.status().klippy.state != wobblescope::moonraker::status::KlippyState::Ready
    })
    .await;
    rig.sim.release_pending(false);
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert!(
        reason(&finished).contains("Timer too close"),
        "{}",
        reason(&finished)
    );
    let (_, meta) = rig.storage.session(&finished.session_id.unwrap()).unwrap();
    assert_eq!(meta.segments.len(), 1);
    assert!(meta.segments[0].window.is_some());
    assert!(
        meta.segments[0]
            .files
            .iter()
            .all(|f| matches!(f.coverage, Coverage::Incomplete { .. }))
    );
    assert_clean(&rig);
}

#[tokio::test]
async fn a_shutdown_while_positioning_before_any_window_leaves_no_session_and_reports_why() {
    let rig = rig().await;
    rig.sim.set_move(Move::Pending);
    let plan = test_plan(&CHIPS, &[Axis::X]);
    let task = start(&rig, &plan).await;
    rig.until("the move", || rig.sim.g1_count() == 1).await;
    rig.sim.printer.set_state("shutdown");
    rig.sim
        .printer
        .set_message("Lost communication with MCU 'mcu'");
    rig.sim.fake.push("notify_klippy_shutdown", json!([]));
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::NoSession);
    assert_eq!(finished.session_id, None);
    assert!(
        reason(&finished).contains("Lost communication"),
        "{}",
        reason(&finished)
    );
    assert!(rig.sessions().is_empty());
    assert!(rig.sim.sweeps().is_empty());
    assert!(rig.leftovers().is_empty());
    assert_clean(&rig);
}

#[tokio::test]
async fn a_lost_control_connection_fails_the_test_keeps_the_data_and_makes_motion_uncertain() {
    let rig = rig().await;
    rig.sim.set_behavior(1, Sweep::Pending);
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let task = start(&rig, &plan).await;
    rig.until("the second sweep", || rig.sim.sweeps().len() == 2)
        .await;
    rig.sim.fake.set_accepting(false);
    rig.sim.fake.close_connections();
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert!(finished.motion_uncertain);
    assert!(
        reason(&finished).contains("unknown"),
        "{}",
        reason(&finished)
    );
    let (_, meta) = rig.storage.session(&finished.session_id.unwrap()).unwrap();
    assert_eq!(meta.segments.len(), 2);
    assert!(rig.slot.quarantine_remaining().is_some());
}

#[tokio::test]
async fn a_chip_that_drops_while_the_sweep_is_pending_waits_for_the_sweep_then_fails_naming_it() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    let plan = test_plan(&CHIPS, &[Axis::X]);
    let task = start(&rig, &plan).await;
    let id = running_id(&rig).await;
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;
    rig.sim.bridge().drop_sensor("ebb36");
    rig.until("waiting for the sweep", || {
        rig.snapshot()
            .test
            .is_some_and(|t| t.phase == Phase::WaitingForSweep)
    })
    .await;
    // The slot is kept and a stop is still accepted while the toolhead may be moving.
    assert!(rig.slot.is_held());
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(rig.snapshot().test.is_some());
    assert!(rig.slot.request_stop(&id));
    rig.sim.release_pending(true);
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    // The stop that was requested while waiting names the end.
    assert_eq!(reason(&finished), "emergency stop");
    assert_clean(&rig);
}

#[tokio::test]
async fn a_chip_that_drops_mid_segment_fails_naming_the_chip_and_closes_the_other_connection() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    let plan = test_plan(&CHIPS, &[Axis::X]);
    let task = start(&rig, &plan).await;
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;
    rig.sim.bridge().drop_sensor("ebb36");
    rig.until("waiting for the sweep", || {
        rig.snapshot()
            .test
            .is_some_and(|t| t.phase == Phase::WaitingForSweep)
    })
    .await;
    rig.sim.release_pending(true);
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert!(
        reason(&finished).contains("lis2dw ebb36"),
        "{}",
        reason(&finished)
    );
    assert_clean(&rig);
}

// ---- failures before and between segments ----

#[tokio::test]
async fn a_positioning_that_cannot_be_corrected_leaves_no_session_and_sends_no_sweep() {
    let rig = rig().await;
    rig.sim.set_move(Move::Frozen);
    rig.sim
        .printer
        .set_field("toolhead", "position", json!([10.0, 20.0, 5.0, 0.0]));
    let plan = test_plan(&CHIPS, &[Axis::X]);
    let task = start(&rig, &plan).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::NoSession);
    let why = reason(&finished);
    assert!(why.contains("150.5") && why.contains("10"), "{why}");
    assert!(rig.sim.sweeps().is_empty());
    assert!(rig.sessions().is_empty());
    assert_clean(&rig);
}

#[tokio::test]
async fn a_print_that_starts_between_segments_fails_before_the_next_positioning() {
    let rig = rig().await;
    let printer = rig.sim.printer.clone();
    rig.sim.on_sweep_done(0, move || {
        printer.set_field("print_stats", "state", json!("printing"));
    });
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let task = start(&rig, &plan).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert!(reason(&finished).contains("print"), "{}", reason(&finished));
    assert_eq!(rig.sim.g1_count(), 1);
    let (_, meta) = rig.storage.session(&finished.session_id.unwrap()).unwrap();
    assert_eq!(meta.segments.len(), 1);
    assert_clean(&rig);
}

#[tokio::test]
async fn the_idle_check_between_segments_waits_for_the_toolhead_to_leave_printing() {
    let rig = rig().await;
    rig.sim.set_idle_delay(Duration::from_millis(300));
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let started = std::time::Instant::now();
    let task = start(&rig, &plan).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    assert!(started.elapsed() >= Duration::from_millis(250));
    assert_eq!(rig.sim.sweeps().len(), 2);
}

#[tokio::test]
async fn a_toolhead_that_never_goes_idle_fails_after_the_wait() {
    let timings = wobblescope::capture::runner::RunnerTimings {
        idle_wait: Duration::from_millis(150),
        ..fast_runner()
    };
    let rig = rig_with(timings, u64::MAX / 4).await;
    rig.sim.set_idle_delay(Duration::from_secs(60));
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let task = start(&rig, &plan).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert!(reason(&finished).contains("idle"), "{}", reason(&finished));
}

// ---- stops around the edges ----

#[tokio::test]
async fn a_stop_before_the_first_sample_arrives_seals_no_session() {
    let rig = rig().await;
    rig.sim.silence_when(None, When::MoveAfterSweeps(0));
    rig.sim.set_behavior(0, Sweep::PendingQuiet);
    let plan = test_plan(&CHIPS, &[Axis::X]);
    let task = start(&rig, &plan).await;
    let id = running_id(&rig).await;
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;
    assert!(rig.slot.request_stop(&id));
    rig.sim.release_pending(false);
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::NoSession);
    assert_eq!(reason(&finished), "emergency stop");
    assert!(rig.sessions().is_empty());
    assert!(rig.leftovers().is_empty());
    assert_clean(&rig);
}

#[tokio::test]
async fn a_stop_right_after_dispatching_segment_three_keeps_one_and_two_and_no_three() {
    let rig = rig().await;
    rig.sim.silence_when(None, When::MoveAfterSweeps(2));
    rig.sim.set_behavior(2, Sweep::PendingQuiet);
    let plan = plan_with_points(
        &[
            [100.0, 100.0, 20.0],
            [150.0, 100.0, 20.0],
            [200.0, 100.0, 20.0],
        ],
        &[Axis::X],
    );
    let task = start(&rig, &plan).await;
    let id = running_id(&rig).await;
    rig.until("the third sweep", || rig.sim.sweeps().len() == 3)
        .await;
    assert!(rig.slot.request_stop(&id));
    rig.sim.release_pending(false);
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    let (_, meta) = rig.storage.session(&id).unwrap();
    assert_eq!(meta.segments.len(), 2);
    assert_clean(&rig);
}

// ---- unknown outcomes ----

#[tokio::test]
async fn a_positioning_call_that_times_out_starts_the_quarantine_and_marks_motion_uncertain() {
    let rig = rig().await;
    rig.sim.set_move(Move::Silent);
    let plan = test_plan(&CHIPS, &[Axis::X]);
    let task = start(&rig, &plan).await;
    rig.until("the move", || rig.sim.g1_count() == 1).await;
    // The toolhead keeps looking busy.
    rig.sim
        .printer
        .set_field("idle_timeout", "state", json!("Printing"));
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::NoSession);
    assert!(finished.motion_uncertain);
    let id = finished.id.clone();
    assert_eq!(rig.slot.motion_uncertain(), Some(id.clone()));
    assert!(rig.slot.quarantine_remaining().is_some());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(rig.slot.motion_uncertain(), Some(id.clone()));

    // Seen idle twice, the marker clears.
    rig.sim
        .printer
        .set_field("idle_timeout", "state", json!("Ready"));
    rig.until("the marker to clear", || {
        rig.slot.motion_uncertain().is_none()
    })
    .await;
    assert!(!rig.snapshot().last.unwrap().motion_uncertain);
}

#[tokio::test]
async fn a_sweep_call_that_times_out_fails_the_test_and_a_new_one_is_refused_by_the_quarantine() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Silent);
    let plan = test_plan(&CHIPS, &[Axis::X]);
    let task = start(&rig, &plan).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert!(finished.motion_uncertain);
    rig.sim
        .printer
        .set_field("idle_timeout", "state", json!("Ready"));
    rig.sim.set_behavior(0, Sweep::Normal);
    match rig.start(&plan).await {
        Err(Refusal::Checks(failures)) => {
            assert!(
                failures.iter().any(|f| f.check == "quarantine"),
                "{failures:?}"
            );
        }
        _ => panic!("expected a quarantine refusal"),
    }
}

// ---- disk ----

#[tokio::test]
async fn a_failed_segment_write_fails_the_test_and_keeps_the_segment_before_it() {
    let rig = rig().await;
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let task = start(&rig, &plan).await;
    let id = running_id(&rig).await;
    // Segment two's first file already exists, so writing it fails.
    let tmp = common::sessions_dir(rig.dir.path()).join(format!(".{id}.tmp"));
    std::fs::write(tmp.join("c0-s1.zst"), b"in the way").unwrap();
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert!(
        reason(&finished).contains("disk write failed"),
        "{}",
        reason(&finished)
    );
    let (_, meta) = rig.storage.session(&id).unwrap();
    assert_eq!(meta.segments.len(), 1);
    assert_clean(&rig);
}

#[tokio::test]
async fn when_even_meta_json_cannot_be_written_the_journal_stays_and_the_next_open_recovers() {
    let rig = rig().await;
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let task = start(&rig, &plan).await;
    let id = running_id(&rig).await;
    let tmp = common::sessions_dir(rig.dir.path()).join(format!(".{id}.tmp"));
    std::fs::write(tmp.join("c0-s1.zst"), b"in the way").unwrap();
    std::fs::create_dir(tmp.join("meta.json.tmp")).unwrap();
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert!(
        reason(&finished).contains("could not seal"),
        "{}",
        reason(&finished)
    );
    assert_eq!(finished.session_id, None);
    assert!(rig.sessions().is_empty());
    assert!(tmp.exists());
    assert_eq!(rig.storage.usage().unwrap().reserved_bytes, 0);

    let (reopened, report) = open(rig.dir.path(), u64::MAX / 4);
    assert!(report.unfinished_capture_found());
    let (_, meta) = reopened.session(&id).unwrap();
    assert_eq!(meta.segments.len(), 1);
    assert!(matches!(meta.outcome, Outcome::Failed { .. }));
}

// ---- a crash ----

#[tokio::test]
async fn killing_the_runner_after_segment_one_leaves_a_session_the_next_open_recovers() {
    let rig = rig().await;
    rig.sim.set_behavior(1, Sweep::Pending);
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let task = start(&rig, &plan).await;
    let id = running_id(&rig).await;
    rig.until("the second sweep", || rig.sim.sweeps().len() == 2)
        .await;
    task.abort();
    let _ = task.await;
    rig.until("the bridges to close", || rig.bridges_open() == 0)
        .await;

    let (reopened, report) = open(rig.dir.path(), u64::MAX / 4);
    assert!(report.unfinished_capture_found());
    let (_, meta) = reopened.session(&id).unwrap();
    assert_eq!(meta.segments.len(), 1);
    assert!(matches!(meta.outcome, Outcome::Failed { .. }));
}

#[test]
fn a_chip_at_its_nominal_rate_gets_no_slow_rate_note() {
    use wobblescope_session::meta::undersampled_note;
    assert_eq!(undersampled_note(Some(1600.0), Some(1610.0)), None);
    assert_eq!(undersampled_note(Some(1600.0), Some(800.0)), None);
    assert_eq!(undersampled_note(None, Some(100.0)), None);
    let note = undersampled_note(Some(1600.0), Some(394.0)).unwrap();
    assert!(note.contains("394 Hz") && note.contains("1600 Hz") && note.contains("firmware"));
}

#[tokio::test]
async fn each_segment_file_records_the_health_of_that_segment_alone() {
    let rig = rig().await;
    // The lis2dw's error counter rises to 3 during the first segment and stays there.
    rig.sim.set_counters("ebb36", 3, 0);
    let plan = test_plan(&CHIPS, &[Axis::X, Axis::Y]);
    let task = start(&rig, &plan).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    let (_, meta) = rig.storage.session(&finished.session_id.unwrap()).unwrap();

    let file = |segment: usize, chip: u32| meta.find_file(chip, segment).unwrap();
    assert_eq!(file(0, 0).health, Some(Health::Ok));
    assert_eq!(file(1, 0).health, Some(Health::Ok));
    assert_eq!(file(0, 1).health, Some(Health::Degraded));
    assert!(
        file(0, 1)
            .health_notes
            .iter()
            .any(|n| n.contains("errors counter rose by 3")),
        "{:?}",
        file(0, 1).health_notes
    );
    // The counter only rose in the first segment, so the second is clean on its own.
    assert_eq!(file(1, 1).health, Some(Health::Ok));
    assert!(file(1, 1).health_notes.is_empty());
    // The chip's overall health is still the worst of its segments.
    assert_eq!(meta.chips[1].health, Health::Degraded);
    assert_eq!(meta.chips[0].health, Health::Ok);
}

// ---- axes-map calibration ----

/// The setup the calibration sends on the printer of [`calibration_rig`].
const CALIBRATION_SETUP: &str = "SET_VELOCITY_LIMIT ACCEL=1500 SQUARE_CORNER_VELOCITY=5 \
                                 MINIMUM_CRUISE_RATIO=0\n\
                                 SET_INPUT_SHAPER SHAPER_FREQ_X=0 SHAPER_FREQ_Y=0";

/// The restore the calibration sends on the printer of [`calibration_rig`].
const CALIBRATION_RESTORE: &str = "SET_VELOCITY_LIMIT VELOCITY=500 ACCEL=3000 \
                                   MINIMUM_CRUISE_RATIO=0.5 SQUARE_CORNER_VELOCITY=8\n\
                                   SET_INPUT_SHAPER SHAPER_TYPE_X=mzv SHAPER_FREQ_X=51.2 \
                                   DAMPING_RATIO_X=0.1 SHAPER_TYPE_Y=mzv SHAPER_FREQ_Y=44 \
                                   DAMPING_RATIO_Y=0.1\n\
                                   G90\n\
                                   RESTORE_GCODE_STATE NAME=wobblescope_calibration";

fn calibration_plan() -> TestPlan {
    let printer = parse_settings(
        &common::fake_printer::fixture("real_settings.json"),
        &common::fake_printer::fixture("real_printer_info.json"),
    )
    .unwrap();
    let request = AxesMapRequest {
        chips: CHIPS
            .iter()
            .map(|c| ChipChoice {
                chip: (*c).to_owned(),
                mounting: Mounting::Toolhead,
            })
            .collect(),
        z_height: None,
        speed: None,
        accel: None,
        travel_speed: None,
    };
    plan_axes_map(&request, &printer, 600).unwrap()
}

/// A rig whose toolhead reports the limits the calibration saves, with `corner_velocity` as its
/// square corner velocity, and whose printer has an input shaper.
async fn calibration_rig(corner_velocity: Option<f64>) -> Rig {
    let rig = rig().await;
    let printer = &rig.sim.printer;
    printer.set_field("toolhead", "max_velocity", json!(500.0));
    printer.set_field("toolhead", "max_accel", json!(3000.0));
    printer.set_field("toolhead", "minimum_cruise_ratio", json!(0.5));
    if let Some(v) = corner_velocity {
        printer.set_field("toolhead", "square_corner_velocity", json!(v));
    }
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
    rig
}

/// A measured move of the calibration, as opposed to its positioning move.
fn is_segment_script(script: &str) -> bool {
    script.contains("G4 P500")
}

fn is_calibration_restore(script: &str) -> bool {
    script.contains("RESTORE_GCODE_STATE NAME=wobblescope_calibration")
}

/// Answers the nth measured move (from 0) with what `answer` returns, and lets every other script
/// run.
fn on_segment_script(
    rig: &Rig,
    nth: usize,
    answer: impl Fn() -> Option<Script> + Send + Sync + 'static,
) {
    let seen = AtomicUsize::new(0);
    rig.sim.intercept(move |script| {
        if !is_segment_script(script) {
            return None;
        }
        (seen.fetch_add(1, Ordering::SeqCst) == nth)
            .then(&answer)
            .flatten()
    });
}

/// The `NAME=` of the line of `script` that starts with `command`.
fn state_name<'a>(script: &'a str, command: &str) -> Option<&'a str> {
    script
        .lines()
        .find_map(|l| l.strip_prefix(command)?.strip_prefix(" NAME="))
}

#[tokio::test]
async fn a_calibration_records_three_segments_and_restores_the_printer() {
    let rig = calibration_rig(Some(8.0)).await;
    let plan = calibration_plan();
    let (_, mut changes) = rig.hub.subscribe();
    let task = start(&rig, &plan).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    assert!(!finished.needs_restart);
    assert!(!finished.motion_uncertain);

    let scripts = rig.sim.scripts();
    assert_eq!(scripts.len(), 7, "{scripts:#?}");
    assert_eq!(scripts[0], "SAVE_GCODE_STATE NAME=wobblescope_calibration");
    assert_eq!(scripts[1], CALIBRATION_SETUP);
    // The travel to the start: the middle of the travel minus 15 mm, at 120 mm/s.
    assert!(
        scripts[2].contains("G1 X135.5 Y139 Z20 F7200") && !is_segment_script(&scripts[2]),
        "{}",
        scripts[2]
    );
    for (i, segment) in plan.segments.iter().enumerate() {
        assert_eq!(scripts[3 + i], plan.segment_script(segment));
    }
    assert_eq!(scripts[6], CALIBRATION_RESTORE);
    assert!(rig.sim.sweeps().is_empty());

    let (_, meta) = rig.storage.session(&finished.session_id.unwrap()).unwrap();
    assert_eq!(meta.kind, SessionKind::AxesMap);
    assert!(meta.axes_map_run.is_some());
    assert_eq!(meta.segments.len(), 3);
    let points: Vec<u32> = meta.segments.iter().map(|s| s.point).collect();
    assert_eq!(points, [0, 1, 2]);
    assert!(meta.segments.iter().all(|s| s.axis == Axis::X));
    assert!(
        meta.segments
            .iter()
            .all(|s| s.window.is_some() && s.files.len() == 2)
    );

    // The live state names the run's kind and no sweep axis or frequency range.
    let mut states = 0;
    while let Ok(snapshot) = changes.try_recv() {
        if let Some(test) = snapshot.test {
            states += 1;
            assert_eq!(test.kind, RunKind::AxesMap);
            assert_eq!(test.axis, None);
            assert_eq!(test.frequency_range, [0.0, 0.0]);
            assert_eq!(test.segments, 3);
        }
    }
    assert!(states > 0);
    assert_clean(&rig);
}

#[tokio::test]
async fn the_restore_uses_the_state_name_that_was_saved() {
    let rig = calibration_rig(Some(8.0)).await;
    let task = start(&rig, &calibration_plan()).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    let scripts = rig.sim.scripts();
    let saved = state_name(scripts.first().unwrap(), "SAVE_GCODE_STATE").unwrap();
    let restored = state_name(scripts.last().unwrap(), "RESTORE_GCODE_STATE").unwrap();
    assert_eq!(saved, restored);
    assert_ne!(saved, wobblescope::hold::settings::STATE_NAME);
    // No other script saves or restores under that name.
    let users = scripts
        .iter()
        .filter(|s| s.contains(&format!("NAME={saved}\n")) || s.ends_with(&format!("NAME={saved}")))
        .count();
    assert_eq!(users, 2, "{scripts:#?}");
}

#[tokio::test]
async fn setup_sets_the_corner_velocity_to_5_and_the_restore_puts_the_saved_value_back() {
    let rig = calibration_rig(Some(12.5)).await;
    let task = start(&rig, &calibration_plan()).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    let scripts = rig.sim.scripts();
    let corner = |script: &str| {
        script
            .lines()
            .find(|l| l.starts_with("SET_VELOCITY_LIMIT"))
            .and_then(|l| {
                l.split_whitespace()
                    .find_map(|w| w.strip_prefix("SQUARE_CORNER_VELOCITY="))
            })
            .map(str::to_owned)
    };
    let first_move = scripts.iter().position(|s| s.contains("G1 ")).unwrap();
    let setup = scripts.iter().position(|s| corner(s).is_some()).unwrap();
    assert!(setup < first_move, "{scripts:#?}");
    assert_eq!(corner(&scripts[setup]).as_deref(), Some("5"));
    let restore = scripts.last().unwrap();
    assert!(is_calibration_restore(restore), "{restore}");
    assert_eq!(corner(restore).as_deref(), Some("12.5"));
}

#[tokio::test]
async fn a_missing_corner_velocity_refuses_the_run_before_any_change() {
    let rig = calibration_rig(None).await;
    let task = start(&rig, &calibration_plan()).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::NoSession);
    assert!(
        reason(&finished).contains("the toolhead's square_corner_velocity is not reported"),
        "{}",
        reason(&finished)
    );
    assert!(!finished.needs_restart);
    // Nothing was saved, changed or moved, so nothing is restored either.
    assert!(rig.sim.scripts().is_empty(), "{:#?}", rig.sim.scripts());
    assert!(rig.sessions().is_empty());
    assert_clean(&rig);
}

#[tokio::test]
async fn a_stop_after_the_first_segment_seals_one_segment_and_still_restores() {
    let rig = calibration_rig(Some(8.0)).await;
    let slot = rig.slot.clone();
    on_segment_script(&rig, 0, move || {
        assert!(slot.request_stop_current().is_some());
        None
    });
    let task = start(&rig, &calibration_plan()).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert_eq!(reason(&finished), "emergency stop");
    assert!(!finished.needs_restart);
    let scripts = rig.sim.scripts();
    assert_eq!(
        scripts.iter().filter(|s| is_segment_script(s)).count(),
        1,
        "{scripts:#?}"
    );
    assert_eq!(scripts.last().unwrap(), CALIBRATION_RESTORE);
    let (_, meta) = rig.storage.session(&finished.session_id.unwrap()).unwrap();
    assert_eq!(meta.kind, SessionKind::AxesMap);
    assert_eq!(meta.segments.len(), 1);
    assert_eq!(meta.segments[0].point, 0);
    assert_clean(&rig);
}

#[tokio::test]
async fn a_lost_connection_during_restore_sets_needs_restart() {
    let rig = calibration_rig(Some(8.0)).await;
    rig.sim
        .intercept(|script| is_calibration_restore(script).then_some(Script::Close));
    let task = start(&rig, &calibration_plan()).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert!(is_calibration_restore(rig.sim.scripts().last().unwrap()));
    assert!(finished.needs_restart, "{finished:?}");
    // The data is still sealed: every segment finished before the restore.
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    let (_, meta) = rig.storage.session(&finished.session_id.unwrap()).unwrap();
    assert_eq!(meta.segments.len(), 3);
    assert_clean(&rig);
}

#[tokio::test]
async fn a_klipper_shutdown_sends_no_restore_and_does_not_set_needs_restart() {
    let rig = calibration_rig(Some(8.0)).await;
    let gate = Gate::new();
    let held = gate.clone();
    on_segment_script(&rig, 1, move || Some(Script::Gated(held.clone())));
    let task = start(&rig, &calibration_plan()).await;
    rig.until("the second segment", || {
        rig.sim
            .scripts()
            .iter()
            .filter(|s| is_segment_script(s))
            .count()
            == 2
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
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert!(
        reason(&finished).contains("Timer too close"),
        "{}",
        reason(&finished)
    );
    assert!(!finished.needs_restart);
    let scripts = rig.sim.scripts();
    assert!(
        !scripts.iter().any(|s| is_calibration_restore(s)),
        "{scripts:#?}"
    );
    let (_, meta) = rig.storage.session(&finished.session_id.unwrap()).unwrap();
    assert_eq!(meta.segments[0].point, 0);
    assert_clean(&rig);
}

#[tokio::test]
async fn a_refused_segment_script_restores_the_saved_state() {
    let rig = calibration_rig(Some(8.0)).await;
    on_segment_script(&rig, 1, || {
        Some(Script::error(
            400,
            "Move out of range: 165.500 169.000 20.000 [0.000]",
        ))
    });
    let task = start(&rig, &calibration_plan()).await;
    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert!(
        reason(&finished).contains("Move out of range"),
        "{}",
        reason(&finished)
    );
    assert!(!finished.needs_restart);
    assert!(!finished.motion_uncertain);
    let scripts = rig.sim.scripts();
    assert_eq!(scripts.last().unwrap(), CALIBRATION_RESTORE, "{scripts:#?}");
    let (_, meta) = rig.storage.session(&finished.session_id.unwrap()).unwrap();
    assert_eq!(meta.segments.len(), 1);
    assert_eq!(meta.segments[0].point, 0);
    assert_clean(&rig);
}
