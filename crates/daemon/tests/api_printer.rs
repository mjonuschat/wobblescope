mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::fake_moonraker::Script;
use common::send;
use common::sim_printer::{Rig, Sweep, rig};
use serde_json::{Value, json};
use wobblescope::capture::live::FinishedOutcome;

fn ui_post(uri: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("x-wobblescope-client", "1")
        .body(Body::empty())
        .unwrap()
}

fn post_json(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("x-wobblescope-client", "1")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn answer(rig: &Rig, request: Request<Body>) -> (StatusCode, Value) {
    let (status, _, body) = send(&rig.app(), request).await;
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn the_printer_state_reports_the_homed_axes() {
    let rig = rig().await;
    rig.sim
        .printer
        .set_field("toolhead", "homed_axes", json!("xy"));
    let (status, body) = answer(&rig, common::request("GET", "/api/printer")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({"homed_axes": "xy", "busy": null, "levelled": null, "level_kind": null, "level_command": null})
    );
}

#[tokio::test]
async fn an_emergency_stop_with_no_test_running_still_goes_to_moonraker() {
    let rig = rig().await;
    rig.sim
        .fake
        .on("printer.emergency_stop", Script::Result(json!("ok")));
    let (status, body) = answer(&rig, ui_post("/api/printer/emergency-stop")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({"dispatched": true}));
    assert_eq!(rig.sim.fake.requests_for("printer.emergency_stop").len(), 1);
}

#[tokio::test]
async fn an_emergency_stop_mid_test_ends_the_test_as_an_emergency_stop() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    let sim = rig.sim.clone();
    rig.sim.fake.on_fn("printer.emergency_stop", move |_, _| {
        sim.release_pending(false);
        Script::Result(json!("ok"))
    });
    rig.sim.feed_until_motion();
    let request =
        json!({"chips": ["adxl345", "lis2dw ebb36"], "axes": ["X", "Y"], "mode": "Pulse"});
    let (status, body) = answer(&rig, post_json("/api/tests", &request)).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;

    let (status, body) = answer(&rig, ui_post("/api/printer/emergency-stop")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let finished = rig.finished().await;
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert_eq!(finished.reason.as_deref(), Some("emergency stop"));
}

#[tokio::test]
async fn a_stop_moonraker_refuses_is_a_503() {
    let rig = rig().await;
    rig.sim
        .fake
        .on("printer.emergency_stop", Script::error(-32601, "refused"));
    let (status, body) = answer(&rig, ui_post("/api/printer/emergency-stop")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"], "moonraker_unavailable");
}

#[tokio::test]
async fn home_runs_g28_and_answers_when_it_is_done() {
    let rig = rig().await;
    rig.sim
        .fake
        .on("printer.gcode.script", Script::Result(json!("ok")));
    let (status, body) = answer(&rig, ui_post("/api/printer/home")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({"homed": true}));
    let calls = rig.sim.fake.requests_for("printer.gcode.script");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["script"], "G28");
}

#[tokio::test]
async fn home_is_refused_while_a_test_runs() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    rig.sim.feed_until_motion();
    let request = json!({"chips": ["adxl345", "lis2dw ebb36"], "axes": ["X"], "mode": "Pulse"});
    let (status, _) = answer(&rig, post_json("/api/tests", &request)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;

    let (status, body) = answer(&rig, ui_post("/api/printer/home")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "preflight_failed");
    assert_eq!(body["failures"][0]["check"], "no_test_running");
}

#[tokio::test]
async fn home_is_refused_while_the_last_test_may_still_be_moving() {
    let rig = rig().await;
    rig.slot.set_motion_uncertain("test-1");
    let (status, body) = answer(&rig, ui_post("/api/printer/home")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["failures"][0]["check"], "no_uncertain_motion");
    assert!(rig.sim.fake.requests_for("printer.gcode.script").is_empty());
    assert!(!rig.slot.is_held());
}

#[tokio::test]
async fn a_home_that_loses_its_answer_keeps_motion_blocked() {
    let rig = rig().await;
    rig.sim.fake.on("printer.gcode.script", Script::Close);
    let (status, _) = answer(&rig, ui_post("/api/printer/home")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(rig.slot.motion_uncertain().as_deref(), Some("homing"));
    assert!(!rig.slot.is_held());
    let (status, body) = answer(&rig, ui_post("/api/printer/home")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["failures"][0]["check"], "no_uncertain_motion");

    let request = json!({"chips": ["adxl345", "lis2dw ebb36"], "axes": ["X"], "mode": "Pulse"});
    let (status, body) = answer(&rig, post_json("/api/tests", &request)).await;
    assert_ne!(status, StatusCode::ACCEPTED, "{body}");
    assert!(rig.slot.quarantine_remaining().is_some());
}

#[tokio::test]
async fn a_home_moonraker_answers_with_an_error_leaves_motion_unblocked() {
    let rig = rig().await;
    rig.sim
        .fake
        .on("printer.gcode.script", Script::error(-32601, "no"));
    let (status, _) = answer(&rig, ui_post("/api/printer/home")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(rig.slot.motion_uncertain().is_none());
}

#[tokio::test]
async fn home_and_level_are_refused_when_the_idle_state_is_not_reported() {
    for uri in ["/api/printer/home", "/api/printer/level"] {
        let rig = rig().await;
        rig.sim.printer.set_field("z_tilt", "applied", json!(false));
        rig.sim
            .printer
            .set_field("idle_timeout", "state", json!(null));
        let (status, body, script) = home_script(&rig, ui_post(uri)).await;
        assert_eq!(status, StatusCode::CONFLICT, "{uri}: {body}");
        assert_eq!(
            body["failures"][0]["check"], "toolhead_idle",
            "{uri}: {body}"
        );
        assert_eq!(script, "", "{uri} sent {script}");
    }
}

#[tokio::test]
async fn home_and_level_are_refused_while_a_print_runs_or_the_toolhead_is_not_idle() {
    for (object, value, check) in [
        ("print_stats", "printing", "not_printing"),
        ("print_stats", "paused", "not_printing"),
        ("idle_timeout", "Printing", "toolhead_idle"),
    ] {
        for uri in ["/api/printer/home", "/api/printer/level"] {
            let rig = rig().await;
            rig.sim.printer.set_field("z_tilt", "applied", json!(false));
            rig.sim.printer.set_field(object, "state", json!(value));
            let (status, body, script) = home_script(&rig, ui_post(uri)).await;
            assert_eq!(status, StatusCode::CONFLICT, "{uri} {value}: {body}");
            assert_eq!(body["failures"][0]["check"], check, "{uri} {value}: {body}");
            assert_eq!(script, "", "{uri} {value} sent {script}");
        }
    }
}

async fn busy_reason(rig: &Rig) -> Value {
    let (status, body) = answer(rig, common::request("GET", "/api/printer")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["busy"].clone()
}

#[tokio::test]
async fn the_printer_state_says_the_printer_is_homing_while_homing_holds_the_slot() {
    let rig = rig().await;
    let _homing = rig.slot.try_acquire_as("homing").unwrap();
    assert_eq!(busy_reason(&rig).await, "The printer is homing.");
}

#[tokio::test]
async fn the_printer_state_says_a_test_or_a_hold_is_running() {
    let rig = rig().await;
    let permit = rig.slot.try_acquire().unwrap();
    permit.begin("test-1");
    assert_eq!(busy_reason(&rig).await, "A test is running.");
    permit.begin("hold-2024");
    assert_eq!(busy_reason(&rig).await, "A hold is running.");
}

#[tokio::test]
async fn the_printer_state_says_the_toolhead_is_moving() {
    let rig = rig().await;
    rig.sim
        .printer
        .set_field("idle_timeout", "state", json!("Printing"));
    assert_eq!(busy_reason(&rig).await, "The toolhead is moving.");
}

#[tokio::test]
async fn a_test_start_while_homing_says_so() {
    let rig = rig().await;
    rig.sim
        .printer
        .set_field("idle_timeout", "state", json!("Printing"));
    let _homing = rig.slot.try_acquire_as("homing").unwrap();
    let request = json!({"chips": ["adxl345", "lis2dw ebb36"], "axes": ["X"], "mode": "Pulse"});
    let (status, body) = answer(&rig, post_json("/api/tests", &request)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let failures = body["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1, "{body}");
    assert!(failures[0]["message"].as_str().unwrap().contains("homing"));
}

async fn home_script(rig: &Rig, request: Request<Body>) -> (StatusCode, Value, String) {
    rig.sim
        .fake
        .on("printer.gcode.script", Script::Result(json!("ok")));
    let before = rig.sim.fake.requests_for("printer.gcode.script").len();
    let (status, body) = answer(rig, request).await;
    let calls = rig.sim.fake.requests_for("printer.gcode.script");
    let script = calls
        .get(before)
        .map(|c| c.params["script"].as_str().unwrap().to_owned())
        .unwrap_or_default();
    (status, body, script)
}

#[tokio::test]
async fn home_sends_the_g28_for_each_requested_axes() {
    for (axes, gcode) in [
        ("all", "G28"),
        ("xy", "G28 X Y"),
        ("x", "G28 X"),
        ("y", "G28 Y"),
        ("z", "G28 Z"),
    ] {
        let rig = rig().await;
        let request = post_json("/api/printer/home", &json!({ "axes": axes }));
        let (status, body, script) = home_script(&rig, request).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(script, gcode, "axes {axes}");
    }
}

#[tokio::test]
async fn home_with_an_empty_json_object_homes_all_axes() {
    let rig = rig().await;
    let (status, _, script) = home_script(&rig, post_json("/api/printer/home", &json!({}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(script, "G28");
}

#[tokio::test]
async fn home_rejects_an_unknown_axes_value_without_touching_the_printer() {
    let rig = rig().await;
    let request = post_json("/api/printer/home", &json!({"axes": "e"}));
    let (status, body, script) = home_script(&rig, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(script.is_empty());
    assert!(!rig.slot.is_held());
}

#[tokio::test]
async fn home_by_axes_still_needs_the_client_header() {
    let rig = rig().await;
    let request = Request::builder()
        .method("POST")
        .uri("/api/printer/home")
        .header("origin", "http://evil.example")
        .header("content-type", "application/json")
        .body(Body::from(json!({"axes": "x"}).to_string()))
        .unwrap();
    let (status, _, script) = home_script(&rig, request).await;
    assert!(status.is_client_error(), "{status}");
    assert!(script.is_empty());
}

#[tokio::test]
async fn home_by_axes_is_refused_while_a_test_runs() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    rig.sim.feed_until_motion();
    let request = json!({"chips": ["adxl345", "lis2dw ebb36"], "axes": ["X"], "mode": "Pulse"});
    let (status, _) = answer(&rig, post_json("/api/tests", &request)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;
    let (status, body) = answer(&rig, post_json("/api/printer/home", &json!({"axes": "z"}))).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
}

fn level_post() -> Request<Body> {
    ui_post("/api/printer/level")
}

async fn state_of(rig: &Rig) -> Value {
    answer(rig, common::request("GET", "/api/printer")).await.1
}

#[tokio::test]
async fn the_printer_state_reports_z_tilt_and_whether_it_is_applied() {
    let rig = rig().await;
    rig.sim.printer.set_field("z_tilt", "applied", json!(false));
    let state = state_of(&rig).await;
    assert_eq!(state["level_kind"], "z_tilt");
    assert_eq!(state["levelled"], false);
    rig.sim.printer.set_field("z_tilt", "applied", json!(true));
    assert_eq!(state_of(&rig).await["levelled"], true);
}

#[tokio::test]
async fn the_printer_state_names_the_command_the_level_button_runs() {
    let rig = rig().await;
    rig.sim.printer.set_field("z_tilt", "applied", json!(false));
    assert_eq!(state_of(&rig).await["level_command"], "Z_TILT_ADJUST");
    let app = rig.app_with_settings(wobblescope::config::Settings {
        level_command: Some("LEVEL_BED".to_owned()),
        ..wobblescope::config::Settings::default()
    });
    let (_, _, body) = send(&app, common::request("GET", "/api/printer")).await;
    let state: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(state["level_command"], "LEVEL_BED");
    assert_eq!(state["level_kind"], "z_tilt");
}

#[tokio::test]
async fn the_printer_state_reports_kalicos_z_tilt_ng_as_z_tilt() {
    let rig = rig().await;
    rig.sim
        .printer
        .set_field("z_tilt_ng", "applied", json!(true));
    let state = state_of(&rig).await;
    assert_eq!(state["level_kind"], "z_tilt");
    assert_eq!(state["levelled"], true);
    assert_eq!(state["level_command"], "Z_TILT_ADJUST");
}

#[tokio::test]
async fn the_printer_state_reports_quad_gantry_level() {
    let rig = rig().await;
    rig.sim
        .printer
        .set_field("quad_gantry_level", "applied", json!(true));
    let state = state_of(&rig).await;
    assert_eq!(state["level_kind"], "quad_gantry_level");
    assert_eq!(state["levelled"], true);
}

#[tokio::test]
async fn level_runs_the_command_for_the_levelling_the_printer_has() {
    for (object, gcode) in [
        ("z_tilt", "Z_TILT_ADJUST"),
        ("z_tilt_ng", "Z_TILT_ADJUST"),
        ("quad_gantry_level", "QUAD_GANTRY_LEVEL"),
    ] {
        let rig = rig().await;
        rig.sim.printer.set_field(object, "applied", json!(false));
        let (status, body, script) = home_script(&rig, level_post()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body, json!({"levelled": true}));
        assert_eq!(script, gcode);
    }
}

#[tokio::test]
async fn level_uses_the_configured_command_instead() {
    let rig = rig().await;
    rig.sim.printer.set_field("z_tilt", "applied", json!(false));
    rig.sim
        .fake
        .on("printer.gcode.script", Script::Result(json!("ok")));
    let app = rig.app_with_settings(wobblescope::config::Settings {
        level_command: Some("LEVEL_BED".to_owned()),
        ..wobblescope::config::Settings::default()
    });
    let (status, _, _) = send(&app, level_post()).await;
    assert_eq!(status, StatusCode::OK);
    let calls = rig.sim.fake.requests_for("printer.gcode.script");
    assert_eq!(calls[0].params["script"], "LEVEL_BED");
}

#[tokio::test]
async fn level_is_refused_when_the_printer_has_no_levelling() {
    let rig = rig().await;
    let (status, body, script) = home_script(&rig, level_post()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(script.is_empty());
    assert!(!rig.slot.is_held());
}

#[tokio::test]
async fn level_is_refused_while_a_test_runs() {
    let rig = rig().await;
    rig.sim.printer.set_field("z_tilt", "applied", json!(false));
    rig.sim.set_behavior(0, Sweep::Pending);
    rig.sim.feed_until_motion();
    let request = json!({"chips": ["adxl345", "lis2dw ebb36"], "axes": ["X"], "mode": "Pulse"});
    let (status, _) = answer(&rig, post_json("/api/tests", &request)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;
    let (status, body) = answer(&rig, level_post()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["failures"][0]["check"], "no_test_running");
}

#[tokio::test]
async fn the_printer_state_says_the_printer_is_levelling_while_levelling_holds_the_slot() {
    let rig = rig().await;
    let _levelling = rig.slot.try_acquire_as("levelling").unwrap();
    assert_eq!(busy_reason(&rig).await, "The printer is levelling.");
}

#[tokio::test]
async fn a_level_that_loses_its_answer_keeps_motion_blocked() {
    let rig = rig().await;
    rig.sim.printer.set_field("z_tilt", "applied", json!(false));
    rig.sim.fake.on("printer.gcode.script", Script::Close);
    let (status, _) = answer(&rig, level_post()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(rig.slot.motion_uncertain().as_deref(), Some("levelling"));
}

fn prepare_post() -> Request<Body> {
    ui_post("/api/printer/prepare")
}

fn scripts_of(rig: &Rig) -> Vec<String> {
    rig.sim
        .fake
        .requests_for("printer.gcode.script")
        .iter()
        .map(|c| c.params["script"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn prepare_homes_levels_then_homes_z() {
    let rig = rig().await;
    rig.sim.printer.set_field("z_tilt", "applied", json!(false));
    rig.sim
        .fake
        .on("printer.gcode.script", Script::Result(json!("ok")));
    let (status, body) = answer(&rig, prepare_post()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({"homed": true, "levelled": true}));
    assert_eq!(scripts_of(&rig), ["G28", "Z_TILT_ADJUST", "G28 Z"]);
    assert!(!rig.slot.is_held());
}

#[tokio::test]
async fn prepare_uses_the_configured_level_command() {
    let rig = rig().await;
    rig.sim.printer.set_field("z_tilt", "applied", json!(false));
    rig.sim
        .fake
        .on("printer.gcode.script", Script::Result(json!("ok")));
    let app = rig.app_with_settings(wobblescope::config::Settings {
        level_command: Some("LEVEL_BED".to_owned()),
        ..wobblescope::config::Settings::default()
    });
    let (status, _, _) = send(&app, prepare_post()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(scripts_of(&rig), ["G28", "LEVEL_BED", "G28 Z"]);
}

#[tokio::test]
async fn prepare_stops_at_the_step_that_fails_and_says_what_was_done() {
    for (failing, expected_scripts, word, note) in [
        (
            0,
            1,
            "homing failed",
            "Levelling and the Z home were not run.",
        ),
        (
            1,
            2,
            "levelling failed",
            "Homing finished; the Z home was not run.",
        ),
        (2, 3, "re-homing Z failed", "Z needs to be homed again."),
    ] {
        let rig = rig().await;
        rig.sim.printer.set_field("z_tilt", "applied", json!(false));
        rig.sim.fake.on_fn("printer.gcode.script", move |_, n| {
            if n == failing {
                Script::Error(400, "Error: out of tolerance")
            } else {
                Script::Result(json!("ok"))
            }
        });
        let (status, body) = answer(&rig, prepare_post()).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
        let message = body["message"].as_str().unwrap();
        assert!(message.contains(word), "{message}");
        assert!(message.contains("out of tolerance"), "{message}");
        assert!(message.contains(note), "{message}");
        assert_eq!(scripts_of(&rig).len(), expected_scripts);
        assert!(!rig.slot.is_held());
    }
}

#[tokio::test]
async fn prepare_is_refused_when_the_printer_has_no_levelling() {
    let rig = rig().await;
    let (status, body) = answer(&rig, prepare_post()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(scripts_of(&rig).is_empty());
}

#[tokio::test]
async fn prepare_is_refused_while_a_test_runs() {
    let rig = rig().await;
    rig.sim.printer.set_field("z_tilt", "applied", json!(false));
    rig.sim.set_behavior(0, Sweep::Pending);
    rig.sim.feed_until_motion();
    let request = json!({"chips": ["adxl345", "lis2dw ebb36"], "axes": ["X"], "mode": "Pulse"});
    let (status, _) = answer(&rig, post_json("/api/tests", &request)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;
    let (status, body) = answer(&rig, prepare_post()).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["failures"][0]["check"], "no_test_running");
}

#[tokio::test]
async fn prepare_holds_the_slot_between_its_steps() {
    let rig = rig().await;
    rig.sim.printer.set_field("z_tilt", "applied", json!(false));
    let slot = rig.slot.clone();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let log = seen.clone();
    rig.sim.fake.on_fn("printer.gcode.script", move |_, _| {
        log.lock().unwrap().push(slot.current_id());
        Script::Result(json!("ok"))
    });
    let (status, _) = answer(&rig, prepare_post()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        *seen.lock().unwrap(),
        [
            Some("homing".to_owned()),
            Some("levelling".to_owned()),
            Some("homing".to_owned())
        ]
    );
}

fn section(motor: &str) -> Value {
    json!({"motor": motor, "tuning_goal": "auto", "tbl": 1, "sgt": 1, "voltage": 24.0,
           "pwm_freq_target": 20000.0})
}

#[tokio::test]
async fn motor_tuning_lists_the_autotune_sections_sorted_by_stepper_name() {
    let rig = rig().await;
    let mut z = section("ldo-42sth40-1684cl350et");
    z["tpfd"] = json!(3);
    rig.sim.printer.set_settings(json!({
        "extruder": {},
        "autotune_tmc stepper_z": z,
        "tmc2240 stepper_z": {"run_current": 1.0},
        "autotune_tmc extruder": section("ldo-36sth20-1004ahg"),
        "tmc2209 extruder": {},
        "autotune_tmc stepper_x": section("ldo-42sth48-2004ac"),
    }));
    let (status, body) = answer(&rig, common::request("GET", "/api/printer/motor-tuning")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let list = body.as_array().unwrap();
    let names: Vec<_> = list
        .iter()
        .map(|e| e["stepper"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["extruder", "stepper_x", "stepper_z"]);
    assert_eq!(list[2]["driver"], "tmc2240");
    assert_eq!(list[2]["motor"], "ldo-42sth40-1684cl350et");
    assert_eq!(list[2]["tpfd"], 3.0);
    assert_eq!(list[2]["voltage"], 24.0);
    assert_eq!(list[0]["driver"], "tmc2209");
    assert_eq!(list[0]["pwm_freq_target"], 20000.0);
}

#[tokio::test]
async fn motor_tuning_leaves_a_missing_driver_and_absent_options_null() {
    let rig = rig().await;
    rig.sim.printer.set_settings(json!({
        "autotune_tmc stepper_y": {"motor": "m"},
        "tmc2240 stepper_x": {},
    }));
    let (_, body) = answer(&rig, common::request("GET", "/api/printer/motor-tuning")).await;
    let list = body.as_array().unwrap();
    assert_eq!(list.len(), 2);
    let entry = &list[1];
    assert_eq!(entry["stepper"], "stepper_y");
    assert_eq!(entry["driver"], Value::Null);
    assert_eq!(entry["motor"], "m");
    for key in [
        "tuning_goal",
        "tpfd",
        "sg4_thrs",
        "overvoltage_vth",
        "voltage",
        "run_current",
        "hold_current",
        "microsteps",
    ] {
        assert_eq!(entry[key], Value::Null, "{key}");
    }
    assert_eq!(list[0]["stepper"], "stepper_x");
    assert_eq!(list[0]["driver"], "tmc2240");
    assert_eq!(list[0]["motor"], Value::Null);
}

#[tokio::test]
async fn motor_tuning_is_empty_without_a_stepper_with_a_driver_or_autotune_section() {
    let rig = rig().await;
    rig.sim.printer.set_settings(json!({"extruder": {}}));
    let (status, body) = answer(&rig, common::request("GET", "/api/printer/motor-tuning")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
}

fn corexy(x: Value, y: Value) -> Value {
    json!({
        "printer": {"kinematics": "corexy"},
        "stepper_x": {"microsteps": 32},
        "stepper_y": {"microsteps": 32},
        "tmc2209 stepper_x": x,
        "tmc2209 stepper_y": y,
        "autotune_tmc stepper_x": section("ldo-42sth48-2004mah"),
        "autotune_tmc stepper_y": section("ldo-42sth48-2004mah"),
    })
}

fn driver(rig: &Rig, stepper: &str, run: f64, hold: f64) {
    let name = format!("tmc2209 {stepper}");
    rig.sim.printer.set_field(&name, "run_current", json!(run));
    rig.sim
        .printer
        .set_field(&name, "hold_current", json!(hold));
}

#[tokio::test]
async fn motor_tuning_gives_the_currents_in_use_to_two_decimals_and_the_microsteps() {
    let rig = rig().await;
    rig.sim
        .printer
        .set_settings(corexy(json!({"hold_current": 2.0}), json!({})));
    driver(
        &rig,
        "stepper_x",
        0.998_268_396_969_243_6,
        0.998_268_396_969_243_6,
    );
    driver(
        &rig,
        "stepper_y",
        0.998_268_396_969_243_6,
        0.998_268_396_969_243_6,
    );
    let (_, body) = answer(&rig, common::request("GET", "/api/printer/motor-tuning")).await;
    assert_eq!(body[0]["run_current"], 1.0);
    assert_eq!(body[0]["hold_current"], 1.0);
    assert_eq!(body[0]["microsteps"], 32);
}

#[tokio::test]
async fn motor_tuning_marks_what_differs_on_the_second_corexy_motor() {
    let rig = rig().await;
    let mut settings = corexy(json!({}), json!({}));
    settings["autotune_tmc stepper_y"]["tuning_goal"] = json!("silent");
    rig.sim.printer.set_settings(settings);
    driver(&rig, "stepper_x", 1.2, 1.2);
    driver(&rig, "stepper_y", 1.0, 0.5);
    let (_, body) = answer(&rig, common::request("GET", "/api/printer/motor-tuning")).await;
    assert_eq!(body[0]["differs"], json!([]));
    assert_eq!(body[0]["differs_from"], Value::Null);
    assert_eq!(body[1]["differs_from"], "stepper_x");
    assert_eq!(
        body[1]["differs"],
        json!(["tuning_goal", "run_current", "hold_current"])
    );
}

#[tokio::test]
async fn motor_tuning_marks_nothing_when_the_motors_match_to_two_decimals() {
    let rig = rig().await;
    rig.sim.printer.set_settings(corexy(json!({}), json!({})));
    driver(&rig, "stepper_x", 0.998, 0.998);
    driver(&rig, "stepper_y", 1.002, 1.002);
    let (_, body) = answer(&rig, common::request("GET", "/api/printer/motor-tuning")).await;
    assert_eq!(body[1]["differs"], json!([]));
    assert_eq!(body[1]["differs_from"], Value::Null);
}

#[tokio::test]
async fn motor_tuning_compares_the_motors_of_a_corexy_printer_only() {
    let rig = rig().await;
    let mut settings = corexy(json!({}), json!({}));
    settings["printer"]["kinematics"] = json!("cartesian");
    rig.sim.printer.set_settings(settings);
    driver(&rig, "stepper_x", 1.2, 1.2);
    driver(&rig, "stepper_y", 1.0, 1.0);
    let (_, body) = answer(&rig, common::request("GET", "/api/printer/motor-tuning")).await;
    assert_eq!(body[1]["differs"], json!([]));
}

#[tokio::test]
async fn motor_tuning_leaves_the_currents_null_without_a_driver_object() {
    let rig = rig().await;
    rig.sim.printer.set_settings(corexy(json!({}), json!({})));
    let (status, body) = answer(&rig, common::request("GET", "/api/printer/motor-tuning")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["run_current"], Value::Null);
    assert_eq!(body[0]["microsteps"], 32);
}

#[tokio::test]
async fn motor_tuning_is_empty_when_klipper_is_not_ready() {
    let rig = rig().await;
    rig.sim.fake.on(
        "printer.objects.query",
        Script::Error(400, "Klippy Disconnected"),
    );
    let (status, body) = answer(&rig, common::request("GET", "/api/printer/motor-tuning")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!([]));
}

const REGISTERS: [&str; 5] = ["CHOPCONF", "PWMCONF", "COOLCONF", "TPWMTHRS", "TCOOLTHRS"];

/// Answers `DUMP_TMC STEPPER=<stepper> REGISTER=<register>` as Klipper does: one console line
/// and then the script's result. A register that is not in `values` is an error, as one that the
/// driver does not have is.
fn serve_registers(rig: &Rig, values: Vec<(&'static str, &'static str, u32)>) {
    rig.sim
        .fake
        .on_fn("printer.gcode.script", move |params, _| {
            let script = params["script"].as_str().unwrap_or_default();
            let word = |key: &str| {
                script
                    .split_whitespace()
                    .find_map(|w| w.strip_prefix(key))
                    .unwrap_or_default()
                    .to_owned()
            };
            let (stepper, register) = (word("STEPPER="), word("REGISTER="));
            match values
                .iter()
                .find(|(s, r, _)| *s == stepper && *r == register)
            {
                Some((_, register, value)) => Script::Notify(
                    vec![(
                        "notify_gcode_response",
                        json!([format!("// {register}:   {value:08x} toff=3 tbl=2")]),
                    )],
                    Box::new(Script::Result(json!("ok"))),
                ),
                None => Script::Error(400, "Unknown register name"),
            }
        });
}

fn dumps(rig: &Rig) -> Vec<String> {
    rig.sim
        .fake
        .requests_for("printer.gcode.script")
        .iter()
        .filter_map(|c| c.params["script"].as_str())
        .filter(|s| s.starts_with("DUMP_TMC"))
        .map(str::to_owned)
        .collect()
}

fn registers_of(stepper: &'static str, base: u32) -> Vec<(&'static str, &'static str, u32)> {
    REGISTERS
        .iter()
        .zip(0..)
        .map(|(register, n)| (stepper, *register, base + n))
        .collect()
}

async fn registers_of_stepper(rig: &Rig, stepper: &str) -> (StatusCode, Value) {
    let uri = format!("/api/printer/motor-tuning/{stepper}/registers");
    answer(rig, common::request("GET", &uri)).await
}

#[tokio::test]
async fn motor_registers_reads_the_five_registers_one_at_a_time_in_order() {
    let rig = rig().await;
    rig.sim.printer.set_settings(corexy(json!({}), json!({})));
    serve_registers(&rig, registers_of("stepper_x", 0x3300_81e0));
    let (status, body) = registers_of_stepper(&rig, "stepper_x").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["stepper"], "stepper_x");
    let names: Vec<_> = body["registers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, REGISTERS);
    assert_eq!(body["registers"][0]["value"], "0x330081e0");
    assert_eq!(body["registers"][4]["value"], "0x330081e4");
    assert_eq!(body["differs_from"], Value::Null);
    assert_eq!(body["differs"], json!([]));
    assert_eq!(
        dumps(&rig),
        REGISTERS
            .iter()
            .map(|r| format!("DUMP_TMC STEPPER=stepper_x REGISTER={r}"))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn motor_registers_leaves_out_a_register_the_driver_does_not_have() {
    let rig = rig().await;
    rig.sim.printer.set_settings(corexy(json!({}), json!({})));
    let mut values = registers_of("stepper_x", 0x10);
    values.retain(|(_, register, _)| *register != "COOLCONF");
    serve_registers(&rig, values);
    let (status, body) = registers_of_stepper(&rig, "stepper_x").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["registers"].as_array().unwrap().len(), 4);
    assert!(
        body["registers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["name"] != "COOLCONF")
    );
}

#[tokio::test]
async fn motor_registers_of_the_second_corexy_motor_say_what_differs_from_the_first() {
    let rig = rig().await;
    rig.sim.printer.set_settings(corexy(json!({}), json!({})));
    let mut values = registers_of("stepper_x", 0x100);
    let mut y = registers_of("stepper_y", 0x100);
    y[1].2 = 0x4f0e_0516;
    values.extend(y);
    serve_registers(&rig, values);
    let (_, body) = registers_of_stepper(&rig, "stepper_y").await;
    assert_eq!(body["differs_from"], "stepper_x");
    assert_eq!(
        body["differs"],
        json!([{"name": "PWMCONF", "other": "0x00000101"}])
    );
    assert_eq!(body["registers"][1]["value"], "0x4f0e0516");
    assert_eq!(dumps(&rig).len(), 10);
}

#[tokio::test]
async fn motor_registers_of_the_first_corexy_motor_are_not_compared() {
    let rig = rig().await;
    rig.sim.printer.set_settings(corexy(json!({}), json!({})));
    let mut values = registers_of("stepper_x", 0x100);
    values.extend(registers_of("stepper_y", 0x900));
    serve_registers(&rig, values);
    let (_, body) = registers_of_stepper(&rig, "stepper_x").await;
    assert_eq!(body["differs"], json!([]));
    assert_eq!(dumps(&rig).len(), 5);
}

#[tokio::test]
async fn motor_registers_are_refused_while_a_print_runs_and_nothing_is_sent() {
    let rig = rig().await;
    rig.sim.printer.set_settings(corexy(json!({}), json!({})));
    serve_registers(&rig, registers_of("stepper_x", 0x10));
    rig.sim
        .printer
        .set_field("print_stats", "state", json!("printing"));
    let (status, body) = registers_of_stepper(&rig, "stepper_x").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(dumps(&rig).is_empty());
}

#[tokio::test]
async fn motor_registers_are_refused_while_a_test_holds_the_slot() {
    let rig = rig().await;
    rig.sim.printer.set_settings(corexy(json!({}), json!({})));
    serve_registers(&rig, registers_of("stepper_x", 0x10));
    let _test = rig.slot.try_acquire_as("a-test").unwrap();
    let (status, _) = registers_of_stepper(&rig, "stepper_x").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(dumps(&rig).is_empty());
}

#[tokio::test]
async fn motor_registers_of_an_unknown_stepper_send_nothing() {
    let rig = rig().await;
    rig.sim.printer.set_settings(corexy(json!({}), json!({})));
    serve_registers(&rig, registers_of("stepper_x", 0x10));
    for name in ["stepper_q", "stepper_x%0AG28", "stepper_x%20REGISTER=GCONF"] {
        let (status, body) = registers_of_stepper(&rig, name).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{name}: {body}");
    }
    assert!(dumps(&rig).is_empty());
}
