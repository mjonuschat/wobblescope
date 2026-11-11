mod common;

use std::time::Duration;

use common::sim_printer::{Rig, rig, rig_with_extra_chips, serve};
use common::test_plan;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use wobblescope::capture::live::FinishedOutcome;
use wobblescope_core::fit::Axis;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(rig: &Rig) -> Socket {
    let addr = serve(rig.app()).await;
    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/api/stream"))
        .await
        .unwrap();
    socket
}

/// A connection whose socket buffers are as small as the system allows (about 400 KB on a macOS
/// loopback), so that a client which stops reading backs the server's writes up soon.
async fn connect_with_small_buffers(rig: &Rig) -> Socket {
    connect_with_small_buffers_to(rig.app()).await
}

async fn connect_with_small_buffers_to(app: axum::Router) -> Socket {
    const BUFFER: u32 = 4096;
    let server = tokio::net::TcpSocket::new_v4().unwrap();
    server.set_send_buffer_size(BUFFER).unwrap();
    server.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let listener = server.listen(8).unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let client = tokio::net::TcpSocket::new_v4().unwrap();
    client.set_recv_buffer_size(BUFFER).unwrap();
    let stream = client.connect(addr).await.unwrap();
    let (socket, _) = tokio_tungstenite::client_async(
        format!("ws://{addr}/api/stream"),
        tokio_tungstenite::MaybeTlsStream::Plain(stream),
    )
    .await
    .unwrap();
    socket
}

/// Everything the socket delivers until it has been quiet for `quiet`.
async fn drain(socket: &mut Socket, quiet: Duration) -> Vec<Message> {
    let mut messages = Vec::new();
    while let Ok(Some(message)) = tokio::time::timeout(quiet, socket.next()).await {
        messages.push(message.unwrap());
    }
    messages
}

fn texts(messages: &[Message]) -> Vec<Value> {
    messages
        .iter()
        .filter_map(|m| match m {
            Message::Text(text) => Some(serde_json::from_str(text).unwrap()),
            _ => None,
        })
        .collect()
}

fn frame_count(messages: &[Message]) -> usize {
    messages
        .iter()
        .filter(|m| matches!(m, Message::Binary(_)))
        .count()
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

#[tokio::test]
async fn a_console_notification_reaches_the_viewer_unchanged() {
    let rig = rig().await;
    let mut socket = connect(&rig).await;
    text_of(&mut socket, "hello").await;
    rig.sim.fake.push("notify_gcode_response", json!(["!! b"]));
    let console = text_of(&mut socket, "console").await;
    assert_eq!(console, json!({"type": "console", "lines": ["!! b"]}));
}

/// Pushes `seconds` of 3200 Hz data to every sensor in `sensors` in real time, 160 samples every
/// 50 ms, so each chip's pipeline cuts a frame on every update interval.
async fn push_in_real_time(rig: &Rig, sensors: &[&str], t0: f64, seconds: f64) {
    for b in 0..(seconds * 20.0) as usize {
        let rows: Vec<[f64; 4]> = (0..160)
            .map(|i| {
                let t = t0 + (b * 160 + i) as f64 / 3200.0;
                [t, (t * 300.0).sin(), 0.0, 9800.0]
            })
            .collect();
        for sensor in sensors {
            rig.sim.bridge().push(sensor, &rows, Some((0, 0)));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn a_slow_reader_gets_dropped_frames_not_a_growing_queue() {
    let rig = rig_with_extra_chips(&["adxl345 b", "adxl345 c", "adxl345 d"]).await;
    let mut socket = connect_with_small_buffers(&rig).await;
    text_of(&mut socket, "hello").await;
    for chip in ["adxl345", "adxl345 b", "adxl345 c", "adxl345 d"] {
        subscribe(&mut socket, chip, 1600.0, 20.0).await;
    }
    rig.until("four bridges open", || rig.bridges_open() == 4)
        .await;

    // The client reads nothing while four chips produce about 800 frames, far more than the
    // socket buffers and the queue hold. Each frame is 16 + 12 * samples bytes. The small socket
    // buffers fill after a few of them, so 10 s is kept as the brief specifies.
    push_in_real_time(&rig, &["adxl345", "b", "c", "d"], 10.0, 10.0).await;

    let messages = drain(&mut socket, Duration::from_millis(500)).await;
    let health = texts(&messages)
        .into_iter()
        .rfind(|m| m["type"] == "health")
        .expect("a health message");
    assert!(health["dropped_frames"].as_u64().unwrap() > 0, "{health}");
    let frames = frame_count(&messages);
    assert!(frames < 700, "{frames} of about 800 frames were delivered");
}

#[tokio::test]
async fn a_viewer_that_stops_reading_is_torn_down_after_the_write_timeout() {
    let rig = rig_with_extra_chips(&["adxl345 b", "adxl345 c", "adxl345 d"]).await;
    let mut socket = connect_with_small_buffers_to(
        rig.app_with_stream_write_timeout(Duration::from_millis(300)),
    )
    .await;
    text_of(&mut socket, "hello").await;
    for chip in ["adxl345", "adxl345 b", "adxl345 c", "adxl345 d"] {
        subscribe(&mut socket, chip, 1600.0, 20.0).await;
    }
    rig.until("four bridges open", || rig.bridges_open() == 4)
        .await;

    // The client reads nothing while data flows until the socket buffers are full and a write
    // blocks. The write timeout then closes the connection and with it every bridge.
    push_in_real_time(&rig, &["adxl345", "b", "c", "d"], 10.0, 10.0).await;
    rig.until("every bridge closed", || rig.bridges_open() == 0)
        .await;
}

#[tokio::test]
async fn a_stalled_reader_with_only_text_traffic_stays_bounded() {
    let rig = rig_with_extra_chips(&["adxl345 b"]).await;
    let mut socket = connect_with_small_buffers(&rig).await;
    text_of(&mut socket, "hello").await;
    subscribe(&mut socket, "adxl345", 200.0, 0.5).await;
    subscribe(&mut socket, "adxl345 b", 200.0, 0.5).await;
    rig.until("both bridges open", || rig.bridges_open() == 2)
        .await;

    // 200 console notifications of 50 KB each, with both chips reporting health.
    let line = "x".repeat(50_000);
    for b in 0..50 {
        let t0 = 10.0 + b as f64 * 0.05;
        let rows: Vec<[f64; 4]> = (0..160)
            .map(|i| [t0 + i as f64 / 3200.0, 0.0, 0.0, 9800.0])
            .collect();
        for sensor in ["adxl345", "b"] {
            rig.sim.bridge().push(sensor, &rows, Some((0, 0)));
        }
        for n in 0..4 {
            let last = b * 4 + n == 199;
            let text = if last {
                "last".to_owned()
            } else {
                line.clone()
            };
            rig.sim.fake.push("notify_gcode_response", json!([text]));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let messages = drain(&mut socket, Duration::from_millis(500)).await;
    assert!(
        !messages.iter().any(|m| matches!(m, Message::Close(_))),
        "the connection was closed"
    );
    let consoles: Vec<Value> = texts(&messages)
        .into_iter()
        .filter(|m| m["type"] == "console")
        .collect();
    assert!(
        consoles
            .iter()
            .all(|c| c["lines"].as_array().unwrap().len() <= 5),
        "a console message held more than 5 lines"
    );
    let lines = consoles.last().unwrap()["lines"].as_array().unwrap();
    assert_eq!(lines.last().unwrap(), "last");
}

#[tokio::test]
async fn a_second_viewer_is_independent() {
    let rig = rig().await;
    let mut wide = connect(&rig).await;
    let mut narrow = connect(&rig).await;
    text(&mut wide).await;
    text(&mut narrow).await;
    subscribe(&mut wide, "adxl345", 200.0, 10.0).await;
    subscribe(&mut narrow, "adxl345", 100.0, 10.0).await;
    rig.until("both bridges open", || {
        rig.sim.bridge().open_on("adxl345") == 2
    })
    .await;
    push_seconds(&rig, "adxl345", 10.0, 1.0);

    let wide_stream = text_of(&mut wide, "stream").await;
    let narrow_stream = text_of(&mut narrow, "stream").await;
    assert_ne!(wide_stream["factor"], narrow_stream["factor"]);
    drain(&mut wide, Duration::from_millis(300)).await;

    drop(narrow);
    rig.until("one bridge closes", || {
        rig.sim.bridge().open_on("adxl345") == 1
    })
    .await;
    push_seconds(&rig, "adxl345", 20.0, 1.0);
    loop {
        // The first push covers t = 10..11 and the second starts at 20, so a frame starting at
        // or after 20 can only come from the second push.
        if let Message::Binary(bytes) = frame(&mut wide).await
            && sample_count(&bytes) > 0
            && f64::from_le_bytes(bytes[4..12].try_into().unwrap()) >= 20.0
        {
            break;
        }
    }
}

#[tokio::test]
async fn a_sweep_and_a_stream_share_a_chip() {
    let rig = rig().await;
    let mut socket = connect(&rig).await;
    text(&mut socket).await;
    let plan = test_plan(&["adxl345", "lis2dw ebb36"], &[Axis::X]);
    let Ok(task) = rig.start(&plan).await else {
        panic!("pre-flight must pass");
    };
    subscribe(&mut socket, "adxl345", 200.0, 10.0).await;

    let finished = rig.finished().await;
    task.await.unwrap();
    assert_eq!(finished.outcome, FinishedOutcome::Complete, "{finished:?}");
    assert_eq!(rig.sessions().len(), 1);
    text_of(&mut socket, "stream").await;

    drop(socket);
    rig.until("the viewer's bridge closes", || rig.bridges_open() == 0)
        .await;
}

fn short_liveness() -> wobblescope::api::StreamTimings {
    wobblescope::api::StreamTimings {
        write_timeout: Duration::from_secs(10),
        ping_interval: Duration::from_millis(100),
        liveness_deadline: Duration::from_millis(400),
    }
}

#[tokio::test]
async fn a_viewer_that_never_answers_pings_is_torn_down() {
    let rig = rig().await;
    let mut socket =
        connect_with_small_buffers_to(rig.app_with_stream_timings(short_liveness())).await;
    text_of(&mut socket, "hello").await;
    subscribe(&mut socket, "adxl345", 200.0, 1.0).await;
    rig.until("the bridge opens", || rig.bridges_open() == 1)
        .await;

    // A trickle that fits the socket buffers, so no write ever blocks. The client stays
    // connected but reads nothing, so it never answers a ping.
    push_seconds(&rig, "adxl345", 10.0, 0.5);
    rig.until("the bridge closes", || rig.bridges_open() == 0)
        .await;
    drop(socket);
}

#[tokio::test]
async fn a_viewer_that_answers_pings_stays_connected() {
    let rig = rig().await;
    let addr = serve(rig.app_with_stream_timings(short_liveness())).await;
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/api/stream"))
        .await
        .unwrap();
    text_of(&mut socket, "hello").await;
    subscribe(&mut socket, "adxl345", 200.0, 1.0).await;
    rig.until("the bridge opens", || rig.bridges_open() == 1)
        .await;

    // Reading is what makes the client answer pings. Run for five times the deadline.
    let messages = drain_for(&mut socket, Duration::from_secs(2)).await;
    assert!(
        !messages.iter().any(|m| matches!(m, Message::Close(_))),
        "the connection was closed"
    );
    assert_eq!(rig.bridges_open(), 1);
}

/// Everything the socket delivers during `total`.
async fn drain_for(socket: &mut Socket, total: Duration) -> Vec<Message> {
    let end = tokio::time::Instant::now() + total;
    let mut messages = Vec::new();
    while let Ok(Some(message)) = tokio::time::timeout_at(end, socket.next()).await {
        messages.push(message.unwrap());
    }
    messages
}
