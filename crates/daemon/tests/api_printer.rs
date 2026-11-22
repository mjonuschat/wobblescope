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
