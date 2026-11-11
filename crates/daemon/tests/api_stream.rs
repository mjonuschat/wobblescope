mod common;

use std::time::Duration;

use common::sim_printer::{Rig, rig, rig_with_extra_chips, serve};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(rig: &Rig) -> Socket {
    let addr = serve(rig.app()).await;
    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/api/stream"))
        .await
        .unwrap();
    socket
}

async fn frame(socket: &mut Socket) -> Message {
    tokio::time::timeout(Duration::from_secs(10), socket.next())
        .await
        .expect("a message in time")
        .expect("the socket stays open")
        .unwrap()
}

/// The next text message, skipping binary frames.
async fn text(socket: &mut Socket) -> Value {
    loop {
        if let Message::Text(text) = frame(socket).await {
            return serde_json::from_str(&text).unwrap();
        }
    }
}

/// The next text message of `kind`, skipping everything else.
async fn text_of(socket: &mut Socket, kind: &str) -> Value {
    loop {
        let message = text(socket).await;
        if message["type"] == kind {
            return message;
        }
    }
}

async fn subscribe(socket: &mut Socket, chip: &str, max_hz: f64, updates: f64) {
    let message = json!({
        "type": "subscribe", "chip": chip, "max_hz": max_hz, "updates_per_second": updates
    });
    socket
        .send(Message::text(message.to_string()))
        .await
        .unwrap();
}

/// Pushes `seconds` of a 3200 Hz signal to `sensor` in batches of 160, starting at `t0`.
fn push_seconds(rig: &Rig, sensor: &str, t0: f64, seconds: f64) {
    let batches = (seconds * 3200.0 / 160.0) as usize;
    for b in 0..batches {
        let rows: Vec<[f64; 4]> = (0..160)
            .map(|i| {
                let t = t0 + (b * 160 + i) as f64 / 3200.0;
                [t, (t * 300.0).sin(), 0.0, 9800.0]
            })
            .collect();
        rig.sim.bridge().push(sensor, &rows, Some((0, 0)));
    }
}

fn sample_count(bytes: &[u8]) -> usize {
    u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize
}

#[tokio::test]
async fn hello_comes_first_and_says_whether_a_print_is_running() {
    let rig = rig().await;
    let mut socket = connect(&rig).await;
    let hello = text(&mut socket).await;
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["printing"], false);
    assert_eq!(hello["defaults"]["updates_per_second"], 10.0);
    assert_eq!(hello["defaults"]["max_hz"], 200.0);
    assert_eq!(hello["limits"]["max_chips"], 4);

    rig.sim
        .printer
        .set_field("print_stats", "state", json!("printing"));
    let mut socket = connect(&rig).await;
    let hello = text(&mut socket).await;
    assert_eq!(hello["printing"], true);
    assert_eq!(hello["defaults"]["updates_per_second"], 5.0);
}

#[tokio::test]
async fn a_subscribe_streams_a_header_then_binary_frames() {
    let rig = rig().await;
    let mut socket = connect(&rig).await;
    text(&mut socket).await;
    subscribe(&mut socket, "adxl345", 200.0, 10.0).await;
    rig.until("the bridge opens", || {
        rig.sim.bridge().open_on("adxl345") == 1
    })
    .await;
    push_seconds(&rig, "adxl345", 10.0, 1.0);

    let stream = text_of(&mut socket, "stream").await;
    assert_eq!(stream["chip"], "adxl345");
    assert_eq!(stream["factor"], 6);
    let mut total = 0;
    while total < 520 {
        if let Message::Binary(bytes) = frame(&mut socket).await {
            total += sample_count(&bytes);
        }
    }
    assert!((total as i64 - 533).abs() <= 2, "{total}");
}

#[tokio::test]
async fn refusals_do_not_change_the_stream() {
    let rig = rig_with_extra_chips(&["adxl345 b", "adxl345 c", "adxl345 d"]).await;
    let mut socket = connect(&rig).await;
    text(&mut socket).await;
    subscribe(&mut socket, "adxl345", 200.0, 10.0).await;
    for chip in ["adxl345 b", "adxl345 c", "adxl345 d"] {
        subscribe(&mut socket, chip, 200.0, 10.0).await;
    }
    rig.until("four bridges open", || rig.bridges_open() == 4)
        .await;

    subscribe(&mut socket, "nothing", 200.0, 10.0).await;
    let refused = text_of(&mut socket, "refused").await;
    assert_eq!(refused["chip"], "nothing");
    subscribe(&mut socket, "lis2dw ebb36", 200.0, 10.0).await;
    let refused = text_of(&mut socket, "refused").await;
    assert_eq!(refused["chip"], "lis2dw ebb36");
    assert_eq!(rig.bridges_open(), 4);

    push_seconds(&rig, "adxl345", 10.0, 1.0);
    text_of(&mut socket, "stream").await;
}

#[tokio::test]
async fn a_second_subscribe_changes_the_settings_without_reopening_the_bridge() {
    let rig = rig().await;
    let mut socket = connect(&rig).await;
    text(&mut socket).await;
    subscribe(&mut socket, "adxl345", 200.0, 10.0).await;
    rig.until("the bridge opens", || {
        rig.sim.bridge().open_on("adxl345") == 1
    })
    .await;
    push_seconds(&rig, "adxl345", 10.0, 1.0);
    let first = text_of(&mut socket, "stream").await;

    subscribe(&mut socket, "adxl345", 100.0, 10.0).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    push_seconds(&rig, "adxl345", 20.0, 1.0);
    let second = text_of(&mut socket, "stream").await;
    assert_ne!(first["factor"], second["factor"]);
    assert_eq!(second["max_hz"], 100.0);
    assert_eq!(rig.sim.bridge().subscribes(), 1);
}

#[tokio::test]
async fn dropping_the_client_aborts_every_bridge_task() {
    let rig = rig().await;
    let mut socket = connect(&rig).await;
    text(&mut socket).await;
    subscribe(&mut socket, "adxl345", 200.0, 10.0).await;
    rig.until("the bridge opens", || {
        rig.sim.bridge().open_on("adxl345") == 1
    })
    .await;
    drop(socket);
    rig.until("the bridge closes", || {
        rig.sim.bridge().open_on("adxl345") == 0
    })
    .await;
}

#[tokio::test]
async fn a_closed_chip_reports_closed_while_the_others_keep_streaming() {
    let rig = rig_with_extra_chips(&["adxl345 b"]).await;
    let mut socket = connect(&rig).await;
    text(&mut socket).await;
    subscribe(&mut socket, "adxl345", 200.0, 10.0).await;
    subscribe(&mut socket, "adxl345 b", 200.0, 10.0).await;
    rig.until("both bridges open", || rig.bridges_open() == 2)
        .await;

    rig.sim.bridge().drop_sensor("adxl345");
    let closed = text_of(&mut socket, "closed").await;
    assert_eq!(closed["chip"], "adxl345");

    push_seconds(&rig, "b", 10.0, 1.0);
    let stream = text_of(&mut socket, "stream").await;
    assert_eq!(stream["chip"], "adxl345 b");
}

#[tokio::test]
async fn a_subscribe_is_refused_while_moonraker_is_not_connected() {
    let rig = rig().await;
    let mut socket = connect(&rig).await;
    text(&mut socket).await;
    rig.sim.fake.set_accepting(false);
    rig.sim.fake.close_connections();
    rig.until("Moonraker to disconnect", || {
        rig.moonraker.control().is_none()
    })
    .await;
    subscribe(&mut socket, "adxl345", 200.0, 10.0).await;
    let refused = text_of(&mut socket, "refused").await;
    assert_eq!(refused["chip"], "adxl345");
    assert_eq!(rig.sim.bridge().open_on("adxl345"), 0);
}
