mod common;

use std::time::Duration;

use common::fake_bridge::Subscribe;
use common::fake_moonraker::{FakeMoonraker, eventually};
use serde_json::Value;
use wobblescope::moonraker::bridge::{Bridge, BridgeError, Event};
use wobblescope::moonraker::tls::Tls;

const DEADLINE: Duration = Duration::from_secs(5);
const OPEN: Duration = Duration::from_millis(500);

fn frames() -> Value {
    let path = format!(
        "{}/tests/data/moonraker/real_bridge_frames.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

async fn open(fake: &FakeMoonraker, sensor: Option<&str>) -> Result<Bridge, BridgeError> {
    Bridge::open(
        &fake.bridge_url("T"),
        "adxl345/dump_adxl345",
        sensor,
        OPEN,
        &Tls::default(),
    )
    .await
}

#[tokio::test]
async fn a_recorded_adxl345_stream_gives_its_batch_and_counters() {
    let fake = FakeMoonraker::start().await;
    let mut bridge = open(&fake, Some("adxl345")).await.unwrap();
    assert_eq!(fake.bridge().tokens(), ["T"]);
    let request = &fake.bridge().requests()[0];
    assert_eq!(request["method"], "adxl345/dump_adxl345");
    assert_eq!(request["params"]["sensor"], "adxl345");

    let f = frames();
    fake.bridge().push_frame("adxl345", &f["adxl345_batch"]);
    let Event::Batch(batch) = bridge.recv().await else {
        panic!("expected a batch");
    };
    assert_eq!(batch.samples.len(), 4);
    assert!((batch.samples.t[0] - 8680.240059).abs() < 1e-9);
    assert!((batch.samples.z[1] - -9652.803275).abs() < 1e-9);
    assert_eq!((batch.errors, batch.overflows), (Some(0), Some(0)));
}

#[tokio::test]
async fn a_batch_without_counters_has_none() {
    let fake = FakeMoonraker::start().await;
    let mut bridge = open(&fake, None).await.unwrap();
    assert!(
        fake.bridge().requests()[0]["params"]
            .get("sensor")
            .is_none()
    );
    fake.bridge().push_frame("", &frames()["counterless_batch"]);
    let Event::Batch(batch) = bridge.recv().await else {
        panic!("expected a batch");
    };
    assert_eq!(batch.samples.len(), 4);
    assert_eq!((batch.errors, batch.overflows), (None, None));
}

#[tokio::test]
async fn klippers_subscribe_errors_come_back_verbatim() {
    let fake = FakeMoonraker::start().await;
    for (name, message) in [
        ("missing_sensor", "Missing Argument [sensor]"),
        ("bad_sensor", "The value 'nope' is not valid for sensor"),
        (
            "unregistered_endpoint",
            "webhooks: No registered callback for path 'adxl345/dump_nothing'",
        ),
    ] {
        let reply = frames()[name]["error"]["message"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(reply, message);
        fake.bridge()
            .on_subscribe(move |_, _| Subscribe::Error(reply.clone()));
        let Err(error) = open(&fake, Some("nope")).await else {
            panic!("{name} opened");
        };
        assert_eq!(error, BridgeError::Subscribe(message.to_owned()), "{name}");
    }
}

#[tokio::test]
async fn an_unanswered_subscription_times_out() {
    let fake = FakeMoonraker::start().await;
    fake.bridge().on_subscribe(|_, _| Subscribe::Silent);
    let Err(error) = open(&fake, Some("adxl345")).await else {
        panic!("opened");
    };
    assert_eq!(error, BridgeError::Timeout);
}

#[tokio::test]
async fn a_socket_dropped_mid_stream_ends_the_stream() {
    let fake = FakeMoonraker::start().await;
    let mut bridge = open(&fake, Some("adxl345")).await.unwrap();
    fake.bridge().drop_sensor("adxl345");
    let event = tokio::time::timeout(DEADLINE, bridge.recv()).await.unwrap();
    assert!(matches!(event, Event::Closed(_)), "{event:?}");
    assert!(matches!(bridge.recv().await, Event::Closed(_)));
}

#[tokio::test]
async fn dropping_the_handle_closes_the_connection() {
    let fake = FakeMoonraker::start().await;
    let bridge = open(&fake, Some("adxl345")).await.unwrap();
    assert_eq!(fake.bridge().open_on("adxl345"), 1);
    drop(bridge);
    eventually("the fake to see the close", DEADLINE, || {
        fake.bridge().open_on("adxl345") == 0
    })
    .await;
}

#[tokio::test]
async fn a_frame_that_is_not_a_batch_ends_the_stream_with_the_reason() {
    let fake = FakeMoonraker::start().await;
    let mut bridge = open(&fake, Some("adxl345")).await.unwrap();
    fake.bridge().push_frame(
        "adxl345",
        &serde_json::json!({"params": {"data": [[1.0, 2.0]]}}),
    );
    let Event::Closed(reason) = tokio::time::timeout(DEADLINE, bridge.recv()).await.unwrap() else {
        panic!("expected the stream to end");
    };
    assert!(reason.contains("not [t, x, y, z]"), "{reason}");
}

#[tokio::test]
async fn recorded_beacon_frames_give_batches_without_counters() {
    let path = format!(
        "{}/tests/data/moonraker/real_beacon_frames.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let recorded: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let fake = FakeMoonraker::start().await;
    let mut bridge = open(&fake, None).await.unwrap();
    fake.bridge().push_frame("", &recorded["beacon_header"]);
    for frame in recorded["beacon_batches"].as_array().unwrap() {
        fake.bridge().push_frame("", frame);
    }
    for _ in 0..2 {
        let Event::Batch(batch) = bridge.recv().await else {
            panic!("expected a batch");
        };
        assert_eq!(batch.samples.len(), 160);
        assert_eq!((batch.errors, batch.overflows), (None, None));
        let rate_hz = 1.0 / (batch.samples.t[1] - batch.samples.t[0]);
        assert!((rate_hz - 3300.0).abs() < 300.0, "{rate_hz}");
    }
}
