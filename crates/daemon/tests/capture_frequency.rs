mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::send;
use common::sim_printer::{Sweep, rig};
use serde_json::{Value, json};

fn post_test(body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/tests")
        .header("x-wobblescope-client", "1")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn the_frequency_klipper_reports_reaches_the_live_state_and_resets_per_segment() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    rig.sim.feed_until_motion();
    let request = json!({"chips": ["adxl345"], "axes": ["X", "Y"], "mode": "Sweep"});
    let (status, _, _) = send(&rig.app(), post_test(&request)).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    rig.until("the first sweep", || rig.sim.sweeps().len() == 1)
        .await;

    // Klipper's console lines come to the daemon as notify_gcode_response.
    rig.sim.fake.push(
        "notify_gcode_response",
        json!(["// Testing axis X", "// Testing frequency 12 Hz"]),
    );
    rig.until("the frequency", || {
        rig.snapshot()
            .test
            .is_some_and(|t| t.frequency_hz == Some(12.0))
    })
    .await;
    rig.sim.fake.push(
        "notify_gcode_response",
        json!(["// Testing frequency 13 Hz"]),
    );
    rig.until("the next frequency", || {
        rig.snapshot()
            .test
            .is_some_and(|t| t.frequency_hz == Some(13.0))
    })
    .await;

    rig.sim.release_pending(true);
    rig.until("the second segment", || {
        rig.snapshot().test.is_some_and(|t| t.segment == 2)
    })
    .await;
    let test = rig.snapshot().test.unwrap();
    assert_eq!(test.frequency_hz, None, "a new segment starts without one");
}
