mod common;

use std::time::Duration;

use common::sim_printer::{Rig, Sweep, rig, serve};
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use wobblescope::capture::live::{FinishedOutcome, FinishedTest};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(addr: std::net::SocketAddr) -> Socket {
    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/api/live"))
        .await
        .unwrap();
    socket
}

async fn next(socket: &mut Socket) -> Value {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(10), socket.next())
            .await
            .expect("a message in time")
            .expect("the socket stays open")
            .unwrap();
        if let Message::Text(text) = frame {
            return serde_json::from_str(&text).unwrap();
        }
    }
}

async fn start(rig: &Rig, addr: std::net::SocketAddr) -> String {
    rig.sim.feed_until_motion();
    reqwest_free_post(addr).await
}

/// POSTs a test through the served app with a raw TCP request.
async fn reqwest_free_post(addr: std::net::SocketAddr) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let body = json!({"chips": ["adxl345", "lis2dw ebb36"], "axes": ["X", "Y"], "mode": "Pulse"})
        .to_string();
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST /api/tests HTTP/1.1\r\nhost: x\r\nx-wobblescope-client: 1\r\ncontent-type: \
         application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 202"), "{response}");
    let json = response.split("\r\n\r\n").nth(1).unwrap();
    serde_json::from_str::<Value>(json).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn the_socket_shows_the_phases_in_order_and_ends_with_the_finished_test() {
    let rig = rig().await;
    let addr = serve(rig.app()).await;
    let mut socket = connect(addr).await;
    let first = next(&mut socket).await;
    assert_eq!(first, json!({"test": null, "last": null}));

    let id = start(&rig, addr).await;
    let mut phases: Vec<String> = Vec::new();
    let last = loop {
        let snapshot = next(&mut socket).await;
        if let Some(test) = snapshot["test"].as_object() {
            assert_eq!(test["id"], id.as_str());
            let phase = test["phase"].as_str().unwrap().to_owned();
            if phases.last() != Some(&phase) {
                phases.push(phase);
            }
        } else if !snapshot["last"].is_null() {
            break snapshot["last"].clone();
        }
    };
    let position = |name: &str| phases.iter().position(|p| p == name);
    assert!(
        position("positioning") < position("measuring"),
        "{phases:?}"
    );
    assert!(position("measuring") < position("draining"), "{phases:?}");
    assert_eq!(last["outcome"], "complete");
    assert_eq!(last["session_id"], id.as_str());
}

#[tokio::test]
async fn a_socket_opened_mid_test_gets_the_running_state_first_and_after_the_end_only_last() {
    let rig = rig().await;
    rig.sim.set_behavior(0, Sweep::Pending);
    let addr = serve(rig.app()).await;
    let id = start(&rig, addr).await;
    rig.until("the sweep", || rig.sim.sweeps().len() == 1).await;

    let mut socket = connect(addr).await;
    let first = next(&mut socket).await;
    assert_eq!(first["test"]["id"], id.as_str());
    assert_eq!(first["test"]["segments"], 2);
    assert!(first["last"].is_null());

    rig.sim.release_pending(true);
    rig.finished().await;
    let mut late = connect(addr).await;
    let snapshot = next(&mut late).await;
    assert!(snapshot["test"].is_null());
    assert_eq!(snapshot["last"]["id"], id.as_str());
}

#[tokio::test]
async fn a_subscriber_that_falls_far_behind_gets_a_fresh_snapshot_not_a_closed_socket() {
    let rig = rig().await;
    let addr = serve(rig.app()).await;
    let mut socket = connect(addr).await;
    let _ = next(&mut socket).await;
    // Far more changes than the broadcast holds, while the client reads nothing.
    for n in 0..1000 {
        rig.hub.update(|s| {
            s.last = Some(FinishedTest {
                id: format!("test-{n}"),
                outcome: FinishedOutcome::Complete,
                reason: None,
                session_id: None,
                motion_uncertain: false,
            });
        });
        if n % 50 == 0 {
            tokio::task::yield_now().await;
        }
    }
    // Reading catches up to the latest state, and the socket is still open.
    loop {
        let snapshot = next(&mut socket).await;
        if snapshot["last"]["id"] == "test-999" {
            break;
        }
    }
}
