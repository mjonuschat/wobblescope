mod common;

use std::time::Duration;

use common::fake_moonraker::{FakeMoonraker, Script, eventually};
use serde_json::json;
use tokio::net::TcpListener;
use wobblescope::moonraker::rpc::{Rpc, RpcError};

const SLOW: Duration = Duration::from_secs(5);
const FAST: Duration = Duration::from_millis(50);

async fn connect(
    fake: &FakeMoonraker,
) -> (
    Rpc,
    tokio::sync::mpsc::UnboundedReceiver<wobblescope::moonraker::rpc::Notification>,
) {
    Rpc::connect(&fake.ws_url(), SLOW).await.unwrap()
}

#[tokio::test]
async fn a_call_returns_the_result_and_carries_jsonrpc_method_and_an_integer_id() {
    let fake = FakeMoonraker::start().await;
    fake.on(
        "server.info",
        Script::Result(json!({"klippy_state": "ready"})),
    );
    let (rpc, _notes) = connect(&fake).await;
    let result = rpc
        .call("server.info", Some(json!({"x": 1})), SLOW)
        .await
        .unwrap();
    assert_eq!(result, json!({"klippy_state": "ready"}));
    let sent = &fake.requests_for("server.info")[0];
    assert_eq!(sent.frame["jsonrpc"], "2.0");
    assert!(sent.frame["id"].is_u64());
    assert_eq!(sent.params, json!({"x": 1}));
}

#[tokio::test]
async fn concurrent_calls_answered_in_reverse_order_each_get_their_own_result() {
    let fake = FakeMoonraker::start().await;
    fake.on(
        "slow",
        Script::after(Duration::from_millis(100), Script::Result(json!("slow"))),
    );
    fake.on("quick", Script::Result(json!("quick")));
    let (rpc, _notes) = connect(&fake).await;
    let (slow, quick) = tokio::join!(rpc.call("slow", None, SLOW), rpc.call("quick", None, SLOW));
    assert_eq!(slow.unwrap(), json!("slow"));
    assert_eq!(quick.unwrap(), json!("quick"));
}

#[tokio::test]
async fn a_notification_pushed_between_calls_arrives_on_the_stream() {
    let fake = FakeMoonraker::start().await;
    let (rpc, mut notes) = connect(&fake).await;
    rpc.call("server.info", None, SLOW).await.unwrap();
    fake.push("notify_klippy_ready", json!([]));
    let note = tokio::time::timeout(SLOW, notes.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(note.method, "notify_klippy_ready");
}

#[tokio::test]
async fn only_the_unauthorized_message_counts_as_unauthorized() {
    let fake = FakeMoonraker::start().await;
    fake.on("a", Script::error(-32602, "Unauthorized"));
    fake.on("b", Script::error(-32602, "Invalid params"));
    let (rpc, _notes) = connect(&fake).await;
    let a = rpc.call("a", None, SLOW).await.unwrap_err();
    let b = rpc.call("b", None, SLOW).await.unwrap_err();
    assert_eq!(
        a,
        RpcError::Moonraker {
            code: -32602,
            message: "Unauthorized".into()
        }
    );
    assert!(a.is_unauthorized());
    assert!(!b.is_unauthorized());
}

#[tokio::test]
async fn a_reply_past_the_timeout_is_a_timeout_error() {
    let fake = FakeMoonraker::start().await;
    fake.on(
        "late",
        Script::after(Duration::from_millis(500), Script::Result(json!(1))),
    );
    let (rpc, _notes) = connect(&fake).await;
    assert_eq!(
        rpc.call("late", None, FAST).await.unwrap_err(),
        RpcError::Timeout
    );
}

#[tokio::test]
async fn a_socket_closed_during_a_call_fails_the_call_and_ends_the_stream() {
    let fake = FakeMoonraker::start().await;
    fake.on("hang", Script::Close);
    let (rpc, mut notes) = connect(&fake).await;
    assert_eq!(
        rpc.call("hang", None, SLOW).await.unwrap_err(),
        RpcError::Closed
    );
    assert!(
        tokio::time::timeout(SLOW, notes.recv())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        rpc.call("again", None, SLOW).await.unwrap_err(),
        RpcError::Closed
    );
}

#[tokio::test]
async fn an_upgrade_answered_with_http_502_is_a_connect_error_naming_502() {
    let fake = FakeMoonraker::start().await;
    fake.refuse_upgrade_with(Some(502));
    let err = Rpc::connect(&fake.ws_url(), SLOW).await.err().unwrap();
    assert!(
        matches!(&err, RpcError::Connect(m) if m.contains("502")),
        "{err:?}"
    );
}

#[tokio::test]
async fn nothing_listening_is_a_connect_error() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let err = Rpc::connect(&format!("ws://127.0.0.1:{port}/websocket"), SLOW)
        .await
        .err()
        .unwrap();
    assert!(matches!(err, RpcError::Connect(_)), "{err:?}");
}

#[tokio::test]
async fn a_listener_that_never_answers_the_upgrade_times_out() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut held = vec![];
        while let Ok((tcp, _)) = listener.accept().await {
            held.push(tcp);
        }
    });
    let started = std::time::Instant::now();
    let err = Rpc::connect(&format!("ws://{addr}/websocket"), FAST)
        .await
        .err()
        .unwrap();
    assert_eq!(err, RpcError::Timeout);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn a_text_frame_that_is_not_json_is_a_protocol_error_and_ends_the_connection() {
    let fake = FakeMoonraker::start().await;
    fake.on("wait", Script::Silent);
    let (rpc, mut notes) = connect(&fake).await;
    let pending = tokio::spawn({
        let rpc = rpc.clone();
        async move { rpc.call("wait", None, SLOW).await }
    });
    eventually("the call to arrive", SLOW, || {
        !fake.requests_for("wait").is_empty()
    })
    .await;
    fake.push_raw("not json");
    let err = pending.await.unwrap().unwrap_err();
    assert!(matches!(err, RpcError::Protocol(_)), "{err:?}");
    assert!(
        tokio::time::timeout(SLOW, notes.recv())
            .await
            .unwrap()
            .is_none()
    );
}
