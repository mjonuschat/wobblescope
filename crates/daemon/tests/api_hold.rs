mod common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use common::fake_moonraker::{Script, eventually};
use common::hold_sim::HoldSim;
use common::sim_printer::serve;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use wobblescope::api::{CaptureState, HoldSocketTimings, ImportGate, mount_configured};
use wobblescope::config::Settings;
use wobblescope::hold::runner::HoldTimings;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn app(sim: &HoldSim, silence: Duration, hold: HoldTimings) -> Router {
    let mut capture = CaptureState::new(600);
    capture.slot = sim.slot.clone();
    capture.hold = HoldSocketTimings {
        keepalive_silence: silence,
        hold,
    };
    mount_configured(
        common::cell(None),
        sim.moonraker.clone(),
        ImportGate::default(),
        capture,
        Settings::default(),
    )
}

async fn connect(app: &Router) -> Socket {
    let addr = serve(app.clone()).await;
    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/api/hold"))
        .await
        .unwrap();
    socket
}

async fn send(socket: &mut Socket, message: Value) {
    socket
        .send(Message::text(message.to_string()))
        .await
        .unwrap();
}

/// The next JSON message, or `None` when the server closed the socket.
async fn next(socket: &mut Socket) -> Option<Value> {
    loop {
        match tokio::time::timeout(Duration::from_secs(10), socket.next())
            .await
            .expect("a message within 10 s")
        {
            Some(Ok(Message::Text(text))) => return Some(serde_json::from_str(&text).unwrap()),
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            _ => return None,
        }
    }
}

/// Reads until a message of `kind` arrives and returns it.
async fn until(socket: &mut Socket, kind: &str) -> Value {
    loop {
        let message = next(socket)
            .await
            .unwrap_or_else(|| panic!("closed before {kind}"));
        if message["type"] == kind {
            return message;
        }
    }
}

fn start(seconds: f64) -> Value {
    json!({"type": "start", "axis": "X", "frequency": 60.0, "accel_per_hz": 60.0,
           "duration_seconds": seconds})
}

#[tokio::test]
async fn start_runs_and_stop_ends() {
    let sim = HoldSim::start().await;
    let app = app(&sim, Duration::from_secs(5), HoldTimings::default());
    let mut socket = connect(&app).await;
    send(&mut socket, start(30.0)).await;
    let started = next(&mut socket).await.unwrap();
    assert_eq!(started["type"], "started");
    assert!(started["id"].as_str().unwrap().starts_with("hold-"));
    let state = until(&mut socket, "state").await;
    assert_eq!(state["frequency"], 60.0);
    send(&mut socket, json!({"type": "stop"})).await;
    let ended = until(&mut socket, "ended").await;
    assert_eq!(ended["reason"], "stopped");
    assert_eq!(ended["restore"], "done");
    assert!(
        next(&mut socket).await.is_none(),
        "the socket closes after the end"
    );
}

#[tokio::test]
async fn a_refused_start_lists_every_failure() {
    let sim = HoldSim::start().await;
    sim.set_print_state("printing");
    sim.set_homed("xy");
    let app = app(&sim, Duration::from_secs(5), HoldTimings::default());
    let mut socket = connect(&app).await;
    send(&mut socket, start(5.0)).await;
    let refused = next(&mut socket).await.unwrap();
    assert_eq!(refused["type"], "refused");
    let checks: Vec<&str> = refused["failures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["check"].as_str().unwrap())
        .collect();
    assert!(
        checks.contains(&"not_printing") && checks.contains(&"homed"),
        "{checks:?}"
    );
    assert!(next(&mut socket).await.is_none());
    assert!(sim.scripts().is_empty());
}

#[tokio::test]
async fn pings_keep_a_hold_alive() {
    let sim = HoldSim::start().await;
    let app = app(&sim, Duration::from_millis(400), HoldTimings::default());
    let mut socket = connect(&app).await;
    send(&mut socket, start(2.0)).await;
    let mut ended = None;
    let deadline = Instant::now() + Duration::from_secs(8);
    while ended.is_none() && Instant::now() < deadline {
        send(&mut socket, json!({"type": "ping"})).await;
        tokio::select! {
            () = tokio::time::sleep(Duration::from_millis(100)) => {}
            message = next(&mut socket) => {
                if let Some(m) = message.filter(|m| m["type"] == "ended") {
                    ended = Some(m);
                }
            }
        }
    }
    assert_eq!(ended.expect("the hold ended")["reason"], "time_limit");
}

#[tokio::test]
async fn a_silent_client_ends_the_hold() {
    let sim = HoldSim::start().await;
    let app = app(&sim, Duration::from_millis(400), HoldTimings::default());
    let mut socket = connect(&app).await;
    send(&mut socket, start(30.0)).await;
    until(&mut socket, "started").await;
    // The client stays connected and says nothing, as when the network is cut silently.
    let silent_since = Instant::now();
    let ended = until(&mut socket, "ended").await;
    assert_eq!(ended["reason"], "connection_lost");
    assert_eq!(ended["restore"], "done");
    let took = silent_since.elapsed();
    assert!(took < Duration::from_millis(400 + 1500 + 500), "{took:?}");
    assert!(
        sim.scripts()
            .last()
            .unwrap()
            .contains("RESTORE_GCODE_STATE")
    );
}

#[tokio::test]
async fn a_closed_socket_ends_the_hold_at_once() {
    let sim = HoldSim::start().await;
    let app = app(&sim, Duration::from_secs(30), HoldTimings::default());
    let mut socket = connect(&app).await;
    send(&mut socket, start(30.0)).await;
    until(&mut socket, "started").await;
    // Close it once the hold is moving, so there is something to put back.
    eventually("a piece", Duration::from_secs(5), || sim.pieces() >= 1).await;
    let closed = Instant::now();
    drop(socket);
    eventually("the restore", Duration::from_millis(2000), || {
        sim.scripts()
            .last()
            .is_some_and(|s| s.contains("RESTORE_GCODE_STATE"))
    })
    .await;
    assert!(closed.elapsed() < Duration::from_millis(2000));
    eventually("the slot to be free", Duration::from_secs(2), || {
        !sim.slot.is_held()
    })
    .await;
}

#[tokio::test]
async fn a_second_hold_is_refused_with_the_start_time() {
    let sim = HoldSim::start().await;
    let app = app(&sim, Duration::from_secs(5), HoldTimings::default());
    let mut first = connect(&app).await;
    send(&mut first, start(30.0)).await;
    until(&mut first, "started").await;
    let mut second = connect(&app).await;
    send(&mut second, start(30.0)).await;
    let refused = next(&mut second).await.unwrap();
    assert_eq!(refused["type"], "refused");
    let failure = refused["failures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["check"] == "no_test_running")
        .expect("a no_test_running failure");
    assert!(
        failure["message"]
            .as_str()
            .unwrap()
            .contains("a hold started"),
        "{failure}"
    );
    send(&mut first, json!({"type": "stop"})).await;
    until(&mut first, "ended").await;
}

#[tokio::test]
async fn an_emergency_stop_reaches_a_hold() {
    let sim = HoldSim::start().await;
    sim.fake
        .on("printer.emergency_stop", Script::Result(json!("ok")));
    let app = app(&sim, Duration::from_secs(5), HoldTimings::default());
    let mut socket = connect(&app).await;
    send(&mut socket, start(30.0)).await;
    let id = until(&mut socket, "started").await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // Once the hold has changed the printer's settings, an emergency stop leaves nothing to restore.
    eventually("a piece", Duration::from_secs(5), || sim.pieces() >= 1).await;
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/tests/{id}/emergency-stop"))
        .header("x-wobblescope-client", "1")
        .body(Body::empty())
        .unwrap();
    let (status, _, body) = common::send(&app, request).await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    let ended = until(&mut socket, "ended").await;
    assert_eq!(ended["reason"], "emergency_stop");
    assert_eq!(ended["restore"], "not_needed");
}

#[tokio::test]
async fn set_changes_the_frequency_live_and_a_misfit_is_rejected() {
    let sim = HoldSim::start().await;
    sim.set_position([300.93, 150.0, 20.0]);
    let app = app(&sim, Duration::from_secs(5), HoldTimings::default());
    let mut socket = connect(&app).await;
    let mut request = start(30.0);
    request["start_point"] = json!([300.93, 150.0, 20.0]);
    send(&mut socket, request).await;
    until(&mut socket, "started").await;
    send(&mut socket, json!({"type": "set", "frequency": 40.0})).await;
    let rejected = until(&mut socket, "rejected").await;
    assert!(
        rejected["message"].as_str().unwrap().contains("axis X"),
        "{rejected}"
    );
    send(&mut socket, json!({"type": "set", "frequency": 70.0})).await;
    loop {
        let state = until(&mut socket, "state").await;
        if state["frequency"].as_f64().unwrap() > 65.0 {
            break;
        }
    }
    send(&mut socket, json!({"type": "stop"})).await;
    let ended = until(&mut socket, "ended").await;
    assert_eq!(ended["reason"], "stopped");
}

#[tokio::test]
async fn a_blocked_script_does_not_hold_up_a_silent_client() {
    let sim = HoldSim::start().await;
    sim.delay_piece(3, Duration::from_secs(3));
    let hold = HoldTimings {
        in_flight_wait: Duration::from_secs(1),
        ..HoldTimings::default()
    };
    let app = app(&sim, Duration::from_millis(400), hold);
    let mut socket = connect(&app).await;
    send(&mut socket, start(30.0)).await;
    until(&mut socket, "started").await;
    // Keep the hold alive until the third piece is blocked, then go silent.
    let deadline = Instant::now() + Duration::from_secs(8);
    while sim.pieces() < 3 {
        assert!(Instant::now() < deadline, "the third piece never came");
        send(&mut socket, json!({"type": "ping"})).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let quiet = Instant::now();
    let ended = until(&mut socket, "ended").await;
    assert_eq!(ended["reason"], "connection_lost");
    let limit = Duration::from_millis(400) + Duration::from_secs(1) + Duration::from_millis(1500);
    assert!(quiet.elapsed() < limit, "{:?}", quiet.elapsed());
}

#[tokio::test]
async fn bad_messages_are_ignored_not_fatal() {
    let sim = HoldSim::start().await;
    let app = app(&sim, Duration::from_secs(5), HoldTimings::default());
    let mut socket = connect(&app).await;
    socket.send(Message::text("not json")).await.unwrap();
    send(&mut socket, json!({"type": "set", "frequency": 50.0})).await;
    send(&mut socket, json!({"type": "stop"})).await;
    send(&mut socket, start(1.0)).await;
    let started = next(&mut socket).await.unwrap();
    assert_eq!(started["type"], "started");
    let ended = until(&mut socket, "ended").await;
    assert_eq!(ended["reason"], "time_limit");
}
