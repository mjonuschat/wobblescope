mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::send;
use common::sim_printer::{Rig, Sweep, rig};
use serde_json::{Value, json};
use wobblescope::capture::live::FinishedOutcome;

fn post(uri: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("x-wobblescope-client", "1")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn ui_post(uri: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("x-wobblescope-client", "1")
        .body(Body::empty())
        .unwrap()
}

fn request() -> Value {
    json!({
        "chips": ["adxl345", "lis2dw ebb36"],
        "axes": ["X", "Y"],
        "mode": "Pulse",
    })
}

async fn answer(rig: &Rig, request: Request<Body>) -> (StatusCode, Value) {
    let (status, _, body) = send(&rig.app(), request).await;
    let value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, value)
}

async fn start_test(rig: &Rig) -> String {
    let (status, body) = answer(rig, post("/api/tests", &request())).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    body["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_started_test_runs_to_a_session_the_session_api_shows() {
    let rig = rig().await;
    rig.sim.feed_until_motion();
    let id = start_test(&rig).await;
    assert!(id.starts_with("test-"));
    let finished = rig.finished().await;
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    assert_eq!(finished.session_id.as_deref(), Some(id.as_str()));

    let (status, body) = answer(&rig, common::request("GET", &format!("/api/sessions/{id}"))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["meta"]["source"], "Capture");
    assert_eq!(body["summary"]["outcome"], "Complete");
}

#[tokio::test]
async fn a_request_failing_three_checks_lists_all_three_and_sends_no_motion() {
    let rig = rig().await;
    rig.sim
        .printer
        .set_field("print_stats", "state", json!("printing"));
    rig.sim
        .printer
        .set_field("toolhead", "homed_axes", json!("xy"));
    rig.sim
        .printer
        .set_field("idle_timeout", "state", json!("Printing"));
    let (status, body) = answer(&rig, post("/api/tests", &request())).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "preflight_failed");
    let checks: Vec<&str> = body["failures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["check"].as_str().unwrap())
        .collect();
    assert_eq!(checks, ["not_printing", "toolhead_idle", "homed"]);
    assert!(rig.sim.scripts().is_empty());
    assert!(!rig.slot.is_held());
}

#[tokio::test]
async fn a_second_test_while_one_runs_is_refused_with_no_test_running() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    rig.sim.feed_until_motion();
    let _id = start_test(&rig).await;
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;
    let (status, body) = answer(&rig, post("/api/tests", &request())).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "preflight_failed");
    assert!(
        body["failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["check"] == "no_test_running")
    );

    // The chip check is refused too.
    let (status, body) = answer(&rig, ui_post("/api/chips/check")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["failures"][0]["check"], "no_test_running");
    rig.sim.release_pending(true);
    rig.finished().await;
}

#[tokio::test]
async fn invalid_overrides_are_a_400_naming_the_field() {
    let rig = rig().await;
    let mut r = request();
    r["overrides"] = json!({"hz_per_sec": 5});
    let (status, body) = answer(&rig, post("/api/tests", &r)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_request");
    assert!(body["message"].as_str().unwrap().starts_with("hz_per_sec:"));
    assert!(rig.sim.scripts().is_empty());
}

#[tokio::test]
async fn an_emergency_stop_mid_test_is_dispatched_at_once_and_ends_the_test_failed() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    // Moonraker's stop shuts Klipper down, which fails the pending script call.
    let sim = rig.sim.clone();
    rig.sim.fake.on_fn("printer.emergency_stop", move |_, _| {
        sim.release_pending(false);
        common::fake_moonraker::Script::Result(json!("ok"))
    });
    rig.sim.feed_until_motion();
    let id = start_test(&rig).await;
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;

    let (status, body) = answer(&rig, ui_post(&format!("/api/tests/{id}/emergency-stop"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body, json!({"dispatched": true}));
    assert_eq!(rig.sim.fake.requests_for("printer.emergency_stop").len(), 1);
    let finished = rig.finished().await;
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    assert_eq!(finished.reason.as_deref(), Some("emergency stop"));
}

#[tokio::test]
async fn a_stop_for_an_unknown_id_or_a_finished_test_is_a_404() {
    let rig = rig().await;
    let (status, body) = answer(&rig, ui_post("/api/tests/nope/emergency-stop")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "test_not_running");

    rig.sim.feed_until_motion();
    let id = start_test(&rig).await;
    rig.finished().await;
    let (status, body) = answer(&rig, ui_post(&format!("/api/tests/{id}/emergency-stop"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "test_not_running");
    assert!(
        rig.sim
            .fake
            .requests_for("printer.emergency_stop")
            .is_empty()
    );
}

#[tokio::test]
async fn a_stop_for_a_finished_test_with_uncertain_motion_is_accepted_and_clears_it() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Silent);
    rig.sim.feed_until_motion();
    let id = start_test(&rig).await;
    let finished = rig.finished().await;
    assert!(finished.motion_uncertain);
    // Keep the toolhead looking busy so the marker does not clear by itself.
    rig.sim
        .printer
        .set_field("idle_timeout", "state", json!("Printing"));

    let (status, body) = answer(&rig, ui_post(&format!("/api/tests/{id}/emergency-stop"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(rig.sim.fake.requests_for("printer.emergency_stop").len(), 1);
    assert!(rig.slot.motion_uncertain().is_none());
    assert!(!rig.snapshot().last.unwrap().motion_uncertain);
}

#[tokio::test]
async fn a_failed_dispatch_of_the_stop_is_reported_and_leaves_the_test_running() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    rig.sim.fake.on(
        "printer.emergency_stop",
        common::fake_moonraker::Script::error(-32601, "refused"),
    );
    rig.sim.feed_until_motion();
    let id = start_test(&rig).await;
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;
    let (status, body) = answer(&rig, ui_post(&format!("/api/tests/{id}/emergency-stop"))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    rig.sim.release_pending(true);
    let finished = rig.finished().await;
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
}

#[tokio::test]
async fn a_stop_that_moonraker_has_not_confirmed_leaves_the_motion_uncertain_and_can_be_retried() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    // Moonraker takes longer than the runner's grace and then refuses.
    rig.sim.fake.on(
        "printer.emergency_stop",
        common::fake_moonraker::Script::after(
            std::time::Duration::from_millis(400),
            common::fake_moonraker::Script::error(-32601, "busy"),
        ),
    );
    rig.sim.feed_until_motion();
    let id = start_test(&rig).await;
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;

    let (status, _) = answer(&rig, ui_post(&format!("/api/tests/{id}/emergency-stop"))).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    let finished = rig.finished().await;
    assert_eq!(finished.outcome, FinishedOutcome::Failed);
    // Klipper never reported shutdown, so the sweep may still be running.
    assert!(finished.motion_uncertain, "{finished:?}");
    assert_eq!(rig.slot.motion_uncertain(), Some(id.clone()));

    // The retry is accepted and goes to Moonraker again.
    rig.sim.fake.on(
        "printer.emergency_stop",
        common::fake_moonraker::Script::Result(json!("ok")),
    );
    let (status, body) = answer(&rig, ui_post(&format!("/api/tests/{id}/emergency-stop"))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(rig.slot.motion_uncertain().is_none());
}
