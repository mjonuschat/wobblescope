mod common;

use std::time::{Duration, Instant};

use common::hold_sim::HoldSim;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::task::JoinHandle;
use wobblescope::hold::pieces::HoldAxis;
use wobblescope::hold::runner::{
    Command, EndReason, Hold, HoldDeps, HoldEnded, HoldEvent, HoldRequest, HoldState, HoldTimings,
    Refusal, begin,
};
use wobblescope::hold::settings::Restore;

fn request(frequency: f64, seconds: Option<f64>) -> HoldRequest {
    HoldRequest {
        axis: HoldAxis::X,
        frequency,
        accel_per_hz: Some(60.0),
        duration_seconds: seconds,
        start_point: None,
    }
}

fn deps(sim: &HoldSim, timings: HoldTimings) -> HoldDeps<'_> {
    HoldDeps {
        moonraker: &sim.moonraker,
        slot: &sim.slot,
        timings,
    }
}

struct Running {
    commands: UnboundedSender<Command>,
    events: UnboundedReceiver<HoldEvent>,
    handle: JoinHandle<HoldEnded>,
}

fn spawn(hold: Hold) -> Running {
    let (commands, command_rx) = unbounded_channel();
    let (event_tx, events) = unbounded_channel();
    let handle = tokio::spawn(hold.run(command_rx, event_tx));
    Running {
        commands,
        events,
        handle,
    }
}

impl Running {
    /// Everything the hold sent so far.
    fn drain(&mut self) -> Vec<HoldEvent> {
        let mut out = vec![];
        while let Ok(e) = self.events.try_recv() {
            out.push(e);
        }
        out
    }

    async fn id(&mut self) -> String {
        loop {
            match self.events.recv().await.expect("the hold started") {
                HoldEvent::Started { id } => return id,
                _ => continue,
            }
        }
    }
}

fn checks(refusal: Refusal) -> Vec<String> {
    match refusal {
        Refusal::Checks(failures) => failures.into_iter().map(|f| f.check).collect(),
        Refusal::Invalid(m) => panic!("expected checks, got invalid: {m}"),
    }
}

#[tokio::test]
async fn a_hold_runs_to_its_time_limit_and_restores() {
    let sim = HoldSim::start().await;
    let hold = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(2.0)),
    )
    .await
    .unwrap();
    let ended = spawn(hold).handle.await.unwrap();
    assert_eq!(ended.reason, EndReason::TimeLimit);
    assert_eq!(ended.restore, Restore::Done);
    assert_eq!(sim.field("toolhead", "max_accel"), 3000.0);
    assert_eq!(sim.field("toolhead", "minimum_cruise_ratio"), 0.5);
    assert_eq!(sim.field("input_shaper", "shaper_freq_x"), 51.2);
    assert_eq!(sim.field("input_shaper", "shaper_freq_y"), 44.0);
    let scripts = sim.scripts();
    assert!(scripts[0].starts_with("SAVE_GCODE_STATE"));
    assert!(scripts[1].starts_with("SET_VELOCITY_LIMIT ACCEL=3600"));
    assert!(scripts.last().unwrap().contains("RESTORE_GCODE_STATE"));
    let moves = sim.piece_moves() as f64;
    assert!(
        (moves - 240.0).abs() <= 24.0,
        "{moves} moves in 2 s at 60 Hz"
    );
}

#[tokio::test]
async fn stop_ends_the_hold_and_restores() {
    let sim = HoldSim::start().await;
    let hold = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(30.0)),
    )
    .await
    .unwrap();
    let running = spawn(hold);
    tokio::time::sleep(Duration::from_secs(1)).await;
    let stop_at = sim.elapsed();
    let sent = Instant::now();
    running.commands.send(Command::Stop).unwrap();
    let ended = running.handle.await.unwrap();
    assert_eq!(ended.reason, EndReason::Stopped);
    assert_eq!(ended.restore, Restore::Done);
    assert!(
        sent.elapsed() < Duration::from_millis(1500),
        "{:?}",
        sent.elapsed()
    );
    for (at, script) in sim.timed_scripts() {
        if script.starts_with("G91\nG1") {
            assert!(
                at <= stop_at + 0.05,
                "a piece at {at} after the stop at {stop_at}"
            );
        }
    }
    assert!(
        sim.scripts()
            .last()
            .unwrap()
            .contains("RESTORE_GCODE_STATE")
    );
}

#[tokio::test]
async fn preflight_reports_every_failure() {
    let sim = HoldSim::start().await;
    sim.set_print_state("printing");
    sim.set_homed("xy");
    let refusal = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(2.0)),
    )
    .await
    .err()
    .unwrap();
    let names = checks(refusal);
    assert!(names.contains(&"not_printing".to_owned()), "{names:?}");
    assert!(names.contains(&"homed".to_owned()), "{names:?}");
    assert!(sim.scripts().is_empty());
}

#[tokio::test]
async fn a_hold_and_a_test_exclude_each_other() {
    let sim = HoldSim::start().await;
    let _permit = sim.slot.try_acquire().unwrap();
    let refusal = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(2.0)),
    )
    .await
    .err()
    .unwrap();
    assert!(checks(refusal).contains(&"no_test_running".to_owned()));
    assert!(sim.scripts().is_empty());
}

#[tokio::test]
async fn a_quarantine_refuses_a_hold() {
    let sim = HoldSim::start().await;
    sim.slot.start_quarantine(Duration::from_secs(30));
    let refusal = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(2.0)),
    )
    .await
    .err()
    .unwrap();
    assert!(checks(refusal).contains(&"quarantine".to_owned()));
    assert!(sim.scripts().is_empty());
}

#[tokio::test]
async fn a_failed_piece_ends_the_hold_and_restores() {
    let sim = HoldSim::start().await;
    sim.fail_when(
        |text, piece| piece == 3 && text.starts_with("G91\nG1"),
        "Move out of range",
    );
    let hold = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(30.0)),
    )
    .await
    .unwrap();
    let ended = spawn(hold).handle.await.unwrap();
    let EndReason::Failed { message } = &ended.reason else {
        panic!("expected a failure, got {:?}", ended.reason);
    };
    assert!(message.contains("Move out of range"), "{message}");
    assert_eq!(ended.restore, Restore::Done);
    assert_eq!(sim.field("toolhead", "max_accel"), 3000.0);
    assert!(
        sim.scripts()
            .last()
            .unwrap()
            .contains("RESTORE_GCODE_STATE")
    );
}

#[tokio::test]
async fn a_failed_restore_reports_what_is_left_and_the_fix() {
    let sim = HoldSim::start().await;
    sim.fail_when(|text, _| text.contains("RESTORE_GCODE_STATE"), "boom");
    let hold = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(1.0)),
    )
    .await
    .unwrap();
    let ended = spawn(hold).handle.await.unwrap();
    let Restore::Failed { left_changed, fix } = ended.restore else {
        panic!("expected a failed restore, got {:?}", ended.restore);
    };
    assert!(left_changed.join("\n").contains("max_accel 3000"));
    assert!(fix.contains("SET_VELOCITY_LIMIT VELOCITY=500 ACCEL=3000 MINIMUM_CRUISE_RATIO=0.5"));
    assert!(fix.contains("RESTORE_GCODE_STATE"));
}

#[tokio::test]
async fn frequency_changes_ramp() {
    let sim = HoldSim::start().await;
    let hold = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(3.5)),
    )
    .await
    .unwrap();
    let mut running = spawn(hold);
    tokio::time::sleep(Duration::from_millis(500)).await;
    running.commands.send(Command::SetFrequency(100.0)).unwrap();
    let ended = (&mut running.handle).await.unwrap();
    assert_eq!(ended.reason, EndReason::TimeLimit);
    let states: Vec<HoldState> = running
        .drain()
        .into_iter()
        .filter_map(|e| match e {
            HoldEvent::State(s) => Some(s),
            _ => None,
        })
        .collect();
    assert!(states.len() >= 6, "{} states", states.len());
    for pair in states.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(b.frequency >= a.frequency - 1e-9, "the frequency fell");
        let allowed = 20.0 * (b.elapsed_seconds - a.elapsed_seconds) + 5.5;
        assert!(b.frequency - a.frequency <= allowed, "{a:?} to {b:?}");
    }
    assert!(
        states.iter().any(|s| s.frequency > 99.0),
        "never reached 100 Hz"
    );
}

#[tokio::test]
async fn until_stopped_is_capped() {
    let sim = HoldSim::start().await;
    let timings = HoldTimings {
        until_stopped_cap_seconds: 1.0,
        ..HoldTimings::default()
    };
    let hold = begin(&deps(&sim, timings), request(60.0, None))
        .await
        .unwrap();
    let began = Instant::now();
    let ended = spawn(hold).handle.await.unwrap();
    assert_eq!(ended.reason, EndReason::TimeLimit);
    assert!(
        began.elapsed() < Duration::from_secs(3),
        "{:?}",
        began.elapsed()
    );
}

#[tokio::test]
async fn nonsense_is_refused_before_any_motion() {
    let sim = HoldSim::start().await;
    let t = HoldTimings::default();
    let invalid = |r: Result<Hold, Refusal>| match r.err().expect("refused") {
        Refusal::Invalid(m) => m,
        Refusal::Checks(c) => panic!("expected invalid, got checks {c:?}"),
    };
    let m = invalid(begin(&deps(&sim, t.clone()), request(300.0, Some(2.0))).await);
    assert!(m.contains("frequency"), "{m}");
    let mut zero = request(60.0, Some(2.0));
    zero.accel_per_hz = Some(0.0);
    let m = invalid(begin(&deps(&sim, t.clone()), zero).await);
    assert!(m.contains("acceleration per Hz"), "{m}");
    let mut edge = request(60.0, Some(2.0));
    edge.start_point = Some([300.995, 150.0, 20.0]);
    let m = invalid(begin(&deps(&sim, t.clone()), edge).await);
    assert!(m.contains("axis X"), "{m}");
    let mut fast = request(60.0, Some(2.0));
    fast.accel_per_hz = Some(2400.0);
    let m = invalid(begin(&deps(&sim, t), fast).await);
    assert!(m.contains("max_velocity"), "{m}");
    assert!(sim.scripts().is_empty());
}

#[tokio::test]
async fn room_is_checked_at_the_start_point() {
    let sim = HoldSim::start().await;
    let mut tight = request(60.0, Some(2.0));
    tight.axis = HoldAxis::Y;
    tight.start_point = Some([150.0, 307.97, 20.0]);
    let refusal = begin(&deps(&sim, HoldTimings::default()), tight)
        .await
        .err()
        .unwrap();
    assert!(matches!(refusal, Refusal::Invalid(m) if m.contains("axis Y")));
    let mut roomy = request(60.0, Some(2.0));
    roomy.axis = HoldAxis::Y;
    roomy.start_point = Some([150.0, 303.0, 20.0]);
    assert!(
        begin(&deps(&sim, HoldTimings::default()), roomy)
            .await
            .is_ok()
    );
    assert!(sim.scripts().is_empty());
}

#[tokio::test]
async fn a_change_that_does_not_fit_is_rejected() {
    let sim = HoldSim::start().await;
    sim.set_position([300.93, 307.97, 20.0]);
    let mut here = request(60.0, Some(3.0));
    here.start_point = Some([300.93, 307.97, 20.0]);
    let hold = begin(&deps(&sim, HoldTimings::default()), here)
        .await
        .unwrap();
    let mut running = spawn(hold);
    tokio::time::sleep(Duration::from_millis(600)).await;
    running
        .commands
        .send(Command::SetAxis(HoldAxis::Y))
        .unwrap();
    running.commands.send(Command::SetFrequency(40.0)).unwrap();
    running.commands.send(Command::SetFrequency(70.0)).unwrap();
    let ended = (&mut running.handle).await.unwrap();
    assert_eq!(ended.reason, EndReason::TimeLimit);
    let events = running.drain();
    let rejected = events
        .iter()
        .filter(|e| matches!(e, HoldEvent::Rejected { .. }))
        .count();
    assert_eq!(rejected, 2, "{events:?}");
    let states: Vec<&HoldState> = events
        .iter()
        .filter_map(|e| match e {
            HoldEvent::State(s) => Some(s),
            _ => None,
        })
        .collect();
    assert!(states.iter().all(|s| s.axis == HoldAxis::X));
    assert!(
        states.iter().all(|s| s.frequency >= 60.0 - 1e-9),
        "40 Hz was applied"
    );
    assert!(
        states.iter().any(|s| s.frequency > 65.0),
        "70 Hz was not applied"
    );
    assert!(sim.scripts().iter().all(|s| !s.contains("G1 Y")));
}

#[tokio::test]
async fn a_blocked_call_does_not_block_stop() {
    for command in [Command::Stop, Command::ConnectionLost] {
        let sim = HoldSim::start().await;
        sim.delay_piece(3, Duration::from_secs(3));
        let timings = HoldTimings {
            in_flight_wait: Duration::from_secs(1),
            ..HoldTimings::default()
        };
        let hold = begin(&deps(&sim, timings.clone()), request(60.0, Some(30.0)))
            .await
            .unwrap();
        let running = spawn(hold);
        common::fake_moonraker::eventually("the third piece", Duration::from_secs(5), || {
            sim.pieces() >= 3
        })
        .await;
        let sent = Instant::now();
        running.commands.send(command.clone()).unwrap();
        let ended = running.handle.await.unwrap();
        let limit = timings.in_flight_wait + Duration::from_millis(1500);
        assert!(
            sent.elapsed() < limit,
            "{:?} for {command:?}",
            sent.elapsed()
        );
        assert!(matches!(
            ended.reason,
            EndReason::Stopped | EndReason::ConnectionLost
        ));
        assert!(
            sim.scripts()
                .last()
                .unwrap()
                .contains("RESTORE_GCODE_STATE")
        );
    }
}

#[tokio::test]
async fn an_emergency_stop_ends_the_hold_without_a_restore() {
    let sim = HoldSim::start().await;
    let hold = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(30.0)),
    )
    .await
    .unwrap();
    let mut running = spawn(hold);
    let id = running.id().await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let sent = Instant::now();
    assert!(sim.slot.request_stop(&id));
    let ended = running.handle.await.unwrap();
    assert!(
        sent.elapsed() < Duration::from_secs(1),
        "{:?}",
        sent.elapsed()
    );
    assert_eq!(ended.reason, EndReason::EmergencyStop);
    assert_eq!(ended.restore, Restore::NotNeeded);
    assert!(
        sim.scripts()
            .iter()
            .all(|s| !s.contains("RESTORE_GCODE_STATE"))
    );
}

#[tokio::test]
async fn the_hold_starts_with_its_id() {
    let sim = HoldSim::start().await;
    let hold = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(1.0)),
    )
    .await
    .unwrap();
    let mut running = spawn(hold);
    let id = running.id().await;
    assert!(id.starts_with("hold-"), "{id}");
    running.handle.await.unwrap();
}

/// The default point of the simulated printer: the middle of X and Y at Z 20.
const DEFAULT_POINT: [f64; 3] = [150.5, 154.0, 20.0];

fn at(point: [f64; 3]) -> HoldRequest {
    HoldRequest {
        start_point: Some(point),
        ..request(60.0, Some(1.0))
    }
}

fn near(a: [f64; 3], b: [f64; 3], tolerance: f64) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() <= tolerance)
}

#[tokio::test]
async fn travel_then_hold() {
    let sim = HoldSim::start().await;
    sim.set_position([110.5, 154.0, 20.0]);
    let hold = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(1.0)),
    )
    .await
    .unwrap();
    let ended = spawn(hold).handle.await.unwrap();
    assert_eq!(ended.reason, EndReason::TimeLimit);
    // 40 mm at the printer's move speed in segments of 0.25 s of motion.
    let travel = sim.travel_scripts();
    assert!(travel.len() >= 3, "{travel:?}");
    assert!(travel.iter().all(|s| s.contains("G1 X") && s.contains('F')));
    let scripts = sim.scripts();
    let first_piece = scripts
        .iter()
        .position(|s| s.starts_with("G91\nG1"))
        .unwrap();
    let last_travel = scripts
        .iter()
        .rposition(|s| s.starts_with("G90\nG1"))
        .unwrap();
    assert!(last_travel < first_piece, "a piece before the travel ended");
    assert!(
        near(sim.position(), DEFAULT_POINT, 0.2),
        "{:?}",
        sim.position()
    );
}

#[tokio::test]
async fn a_hold_at_the_point_does_not_travel() {
    let sim = HoldSim::start().await;
    let hold = begin(
        &deps(&sim, HoldTimings::default()),
        request(60.0, Some(1.0)),
    )
    .await
    .unwrap();
    spawn(hold).handle.await.unwrap();
    assert!(sim.travel_scripts().is_empty());
}

#[tokio::test]
async fn a_missed_point_gets_one_correction() {
    let sim = HoldSim::start().await;
    sim.set_position([140.5, 154.0, 20.0]);
    sim.set_landing_error([0.2, 0.0, 0.0]);
    let hold = begin(&deps(&sim, HoldTimings::default()), at(DEFAULT_POINT))
        .await
        .unwrap();
    let ended = spawn(hold).handle.await.unwrap();
    assert_eq!(ended.reason, EndReason::TimeLimit);
    // 10 mm is one segment at 0.25 s and 50 mm/s (12.5 mm), then one correction.
    assert_eq!(sim.travel_scripts().len(), 2, "{:?}", sim.travel_scripts());
    assert!(
        near(sim.position(), DEFAULT_POINT, 0.06),
        "{:?}",
        sim.position()
    );
}

#[tokio::test]
async fn the_position_is_read_after_the_travel_finishes() {
    let sim = HoldSim::start().await;
    sim.set_position([60.5, 154.0, 20.0]);
    let hold = begin(&deps(&sim, HoldTimings::default()), at(DEFAULT_POINT))
        .await
        .unwrap();
    let ended = spawn(hold).handle.await.unwrap();
    assert_eq!(ended.reason, EndReason::TimeLimit);
    // 90 mm is 8 segments of 12.5 mm at the printer's move speed. The last one finishes about a
    // second after the script is accepted, and a position read before that would look like a
    // miss and send a correction.
    assert_eq!(sim.travel_scripts().len(), 8, "{:?}", sim.travel_scripts());
    assert!(near(sim.position(), DEFAULT_POINT, 0.06));
}

#[tokio::test]
async fn live_changes_use_the_position_after_travel() {
    let sim = HoldSim::start().await;
    sim.set_position([150.0, 150.0, 20.0]);
    // The stated point has 0.04 mm of room on Y, less than the 0.0625 mm a Y hold needs. The
    // toolhead lands 0.04 mm short of it, still within the position tolerance, and there it has
    // 0.08 mm: the Y change fits where the toolhead is.
    sim.set_landing_error([0.0, -0.04, 0.0]);
    let mut tight = at([150.0, 307.96, 20.0]);
    tight.duration_seconds = Some(5.0);
    let hold = begin(&deps(&sim, HoldTimings::default()), tight)
        .await
        .unwrap();
    let mut running = spawn(hold);
    tokio::time::sleep(Duration::from_millis(4500)).await;
    running
        .commands
        .send(Command::SetAxis(HoldAxis::Y))
        .unwrap();
    let ended = (&mut running.handle).await.unwrap();
    assert_eq!(ended.reason, EndReason::TimeLimit);
    let events = running.drain();
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, HoldEvent::Rejected { .. })),
        "the change was checked against the stated point, not where the toolhead is: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, HoldEvent::State(s) if s.axis == HoldAxis::Y))
    );
}

#[tokio::test]
async fn stop_during_travel_ends_between_segments() {
    let sim = HoldSim::start().await;
    sim.set_position([20.5, 154.0, 20.0]);
    let hold = begin(&deps(&sim, HoldTimings::default()), at(DEFAULT_POINT))
        .await
        .unwrap();
    let running = spawn(hold);
    tokio::time::sleep(Duration::from_millis(900)).await;
    let sent = Instant::now();
    running.commands.send(Command::Stop).unwrap();
    let ended = running.handle.await.unwrap();
    assert_eq!(ended.reason, EndReason::Stopped);
    assert_eq!(ended.restore, Restore::Done);
    assert!(sent.elapsed() < Duration::from_millis(1500));
    // 130 mm is 11 segments of 12.5 mm, and the stop came well before they were all sent.
    assert!(
        sim.travel_scripts().len() < 11,
        "{}",
        sim.travel_scripts().len()
    );
    assert_eq!(sim.pieces(), 0);
    assert!(
        sim.scripts()
            .last()
            .unwrap()
            .contains("RESTORE_GCODE_STATE")
    );
}

#[tokio::test]
async fn an_axis_change_is_checked_at_the_frequency_the_ramp_has_reached() {
    let sim = HoldSim::start().await;
    sim.set_position([150.0, 307.9, 20.0]);
    // Y has 0.1 mm of room: enough for a 60 Hz hold (0.0625 mm), not for the 0.1875 mm a 20 Hz
    // hold needs. The ramp to 60 Hz has not got there yet when the axis changes.
    let mut slow = at([150.0, 307.9, 20.0]);
    slow.frequency = 20.0;
    slow.duration_seconds = Some(3.0);
    let hold = begin(&deps(&sim, HoldTimings::default()), slow)
        .await
        .unwrap();
    let mut running = spawn(hold);
    tokio::time::sleep(Duration::from_millis(300)).await;
    running.commands.send(Command::SetFrequency(60.0)).unwrap();
    running
        .commands
        .send(Command::SetAxis(HoldAxis::Y))
        .unwrap();
    let ended = (&mut running.handle).await.unwrap();
    assert_eq!(ended.reason, EndReason::TimeLimit);
    let events = running.drain();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, HoldEvent::Rejected { .. })),
        "the axis change should be rejected while the ramp is below 60 Hz"
    );
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, HoldEvent::State(s) if s.axis == HoldAxis::Y)),
        "no piece may go out on Y at the ramping frequency"
    );
}
