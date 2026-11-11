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
    assert_eq!(body, json!({"homed_axes": "xy"}));
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
