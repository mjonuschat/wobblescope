mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::fake_moonraker::{FakeMoonraker, Script};
use common::fake_printer::FakePrinter;
use serde_json::json;
use wobblescope::capture::position::{
    PositionError, SWEEP_LEAD_IN_SECONDS, Timeouts, move_to_point, window_end, window_start,
};
use wobblescope::moonraker::rpc::Rpc;
use wobblescope::moonraker::tls::Tls;

const POINT: [f64; 3] = [150.5, 154.0, 20.0];

fn timeouts() -> Timeouts {
    Timeouts {
        script: Duration::from_millis(300),
        query: Duration::from_millis(300),
    }
}

struct Rig {
    fake: FakeMoonraker,
    printer: FakePrinter,
    rpc: Rpc,
    // Keeps the notification stream open.
    _notes: tokio::sync::mpsc::UnboundedReceiver<wobblescope::moonraker::rpc::Notification>,
}

async fn rig() -> Rig {
    let fake = FakeMoonraker::start().await;
    let printer = FakePrinter::new(&fake, "ready", json!({}));
    let (rpc, notes) = Rpc::connect(&fake.ws_url(), Duration::from_secs(2), &Tls::default())
        .await
        .unwrap();
    Rig {
        fake,
        printer,
        rpc,
        _notes: notes,
    }
}

/// The scripts the fake received, in order.
fn scripts(rig: &Rig) -> Vec<String> {
    rig.fake
        .requests_for("printer.gcode.script")
        .iter()
        .map(|r| r.params["script"].as_str().unwrap().to_owned())
        .collect()
}

fn g1_count(rig: &Rig) -> usize {
    scripts(rig).iter().filter(|s| s.contains("G1 ")).count()
}

/// Makes `printer.gcode.script` move the simulated toolhead to the commanded point plus `offset`.
fn simulate(rig: &Rig, offset: [f64; 3]) {
    let printer = rig.printer.clone();
    rig.fake.on_fn("printer.gcode.script", move |params, _| {
        let script = params["script"].as_str().unwrap_or_default();
        if let Some(line) = script.lines().find(|l| l.starts_with("G1 ")) {
            let mut at = [0.0; 3];
            for word in line.split_whitespace() {
                let value = word[1..].parse::<f64>().ok();
                match (word.as_bytes()[0], value) {
                    (b'X', Some(v)) => at[0] = v,
                    (b'Y', Some(v)) => at[1] = v,
                    (b'Z', Some(v)) => at[2] = v,
                    _ => {}
                }
            }
            printer.set_field(
                "toolhead",
                "position",
                json!([at[0] + offset[0], at[1] + offset[1], at[2] + offset[2], 0.0]),
            );
        }
        Script::Result(json!("ok"))
    });
}

fn ready() -> bool {
    true
}

#[tokio::test]
async fn a_toolhead_that_lands_on_the_point_needs_no_correction() {
    let rig = rig().await;
    simulate(&rig, [0.0; 3]);
    move_to_point(&rig.rpc, POINT, 50.0, timeouts(), ready)
        .await
        .unwrap();
    assert_eq!(g1_count(&rig), 1);
    assert!(scripts(&rig)[0].contains("G1 X150.5 Y154 Z20 F3000"));
}

#[tokio::test]
async fn an_offset_toolhead_gets_one_corrective_move_aimed_the_other_way() {
    let rig = rig().await;
    simulate(&rig, [1.0, 2.0, 0.3]);
    move_to_point(&rig.rpc, POINT, 50.0, timeouts(), ready)
        .await
        .unwrap();
    let scripts = scripts(&rig);
    assert_eq!(scripts.len(), 2);
    assert!(
        scripts[1].contains("G1 X149.5 Y152 Z19.7 "),
        "{}",
        scripts[1]
    );
}

#[tokio::test]
async fn forty_micrometres_is_accepted_and_sixty_is_corrected() {
    let rig = rig().await;
    simulate(&rig, [0.04, 0.0, 0.0]);
    move_to_point(&rig.rpc, POINT, 50.0, timeouts(), ready)
        .await
        .unwrap();
    assert_eq!(g1_count(&rig), 1);

    let rig = self::rig().await;
    simulate(&rig, [0.0, 0.06, 0.0]);
    move_to_point(&rig.rpc, POINT, 50.0, timeouts(), ready)
        .await
        .unwrap();
    assert_eq!(g1_count(&rig), 2);
}

#[tokio::test]
async fn still_off_after_the_correction_is_unreachable_with_both_positions_and_two_moves() {
    let rig = rig().await;
    // The toolhead reports the same wrong position whatever it is told.
    rig.printer
        .set_field("toolhead", "position", json!([10.0, 20.0, 5.0, 0.0]));
    let Err(PositionError::Unreachable { wanted, got }) =
        move_to_point(&rig.rpc, POINT, 50.0, timeouts(), ready).await
    else {
        panic!("expected unreachable");
    };
    assert_eq!(wanted, POINT);
    assert_eq!(got, [10.0, 20.0, 5.0]);
    assert_eq!(g1_count(&rig), 2);
}

#[tokio::test]
async fn every_moving_script_saves_sets_absolute_moves_waits_and_restores_in_that_order() {
    let rig = rig().await;
    simulate(&rig, [0.5, 0.0, 0.0]);
    move_to_point(&rig.rpc, POINT, 50.0, timeouts(), ready)
        .await
        .unwrap();
    for script in scripts(&rig) {
        let at = |needle: &str| script.find(needle).unwrap_or_else(|| panic!("{needle}"));
        assert!(at("SAVE_GCODE_STATE") < at("G90"));
        assert!(at("G90") < at("G1 "));
        assert!(at("G1 ") < at("M400"));
        assert!(at("M400") < at("RESTORE_GCODE_STATE"));
    }
}

#[tokio::test]
async fn a_script_that_gets_no_answer_is_unknown_not_unreachable() {
    let rig = rig().await;
    rig.fake.on("printer.gcode.script", Script::Silent);
    let result = move_to_point(&rig.rpc, POINT, 50.0, timeouts(), ready).await;
    assert!(
        matches!(result, Err(PositionError::Unknown(_))),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_gcode_error_while_ready_is_followed_by_a_separate_restore() {
    let rig = rig().await;
    let calls = Arc::new(Mutex::new(0));
    let seen = calls.clone();
    rig.fake.on_fn("printer.gcode.script", move |_, n| {
        *seen.lock().unwrap() = n + 1;
        if n == 0 {
            Script::error(400, "Move out of range: 150.500 400.000 20.000 [0.000]")
        } else {
            Script::Result(json!("ok"))
        }
    });
    let result = move_to_point(&rig.rpc, POINT, 50.0, timeouts(), ready).await;
    let Err(PositionError::Klipper(message)) = result else {
        panic!("expected a Klipper error");
    };
    assert!(message.contains("Move out of range"));
    let scripts = scripts(&rig);
    assert_eq!(scripts.len(), 2);
    assert_eq!(scripts[1], "RESTORE_GCODE_STATE NAME=wobblescope");
}

#[tokio::test]
async fn a_gcode_error_in_shutdown_sends_no_restore() {
    let rig = rig().await;
    rig.fake.on(
        "printer.gcode.script",
        Script::error(400, "Klipper state: Shutdown"),
    );
    let result = move_to_point(&rig.rpc, POINT, 50.0, timeouts(), || false).await;
    assert!(matches!(result, Err(PositionError::Klipper(_))));
    assert_eq!(scripts(&rig).len(), 1);
}

#[tokio::test]
async fn the_window_start_is_the_larger_of_print_time_and_estimated_print_time() {
    let rig = rig().await;
    rig.printer
        .set_field("toolhead", "print_time", json!(1496.29));
    rig.printer
        .set_field("toolhead", "estimated_print_time", json!(1523.4));
    let q = Duration::from_secs(1);
    // The window starts one lead-in after the mark, which is the later of the two times.
    let lead = SWEEP_LEAD_IN_SECONDS;
    let start = |t: f64| (t + lead).to_bits();
    assert_eq!(
        window_start(&rig.rpc, q).await.unwrap().to_bits(),
        start(1523.4)
    );
    rig.printer
        .set_field("toolhead", "print_time", json!(1530.0));
    assert_eq!(
        window_start(&rig.rpc, q).await.unwrap().to_bits(),
        start(1530.0)
    );
    assert_eq!(window_end(&rig.rpc, q).await.unwrap(), 1530.0);
}
