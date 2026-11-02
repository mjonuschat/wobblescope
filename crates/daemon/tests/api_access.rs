mod common;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use common::{cell, open, request, send};
use futures_util::StreamExt;
use serde_json::{Value, json};
use wobblescope::access::policy::Net;
use wobblescope::access::{AccessState, Policy};
use wobblescope::api::{AccessGuard, guarded, mount};
use wobblescope::moonraker::{Moonraker, Publisher, channel};

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/data/moonraker/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn real_policy() -> Policy {
    Policy::build(&fixture("real_server_config.json"), false, Some("buster")).unwrap()
}

struct Rig {
    publisher: Publisher,
    moonraker: Moonraker,
    _data: tempfile::TempDir,
    app: Router,
}

fn rig(proxies: &[&str], hosts: &[&str]) -> Rig {
    let data = tempfile::tempdir().unwrap();
    let (storage, _) = open(data.path(), 1 << 30);
    let (publisher, moonraker) = channel();
    let app = mount(cell(Some(storage)), moonraker.clone()).fallback(|| async { "the web ui" });
    let guard = AccessGuard::new(
        moonraker.clone(),
        proxies.iter().map(|p| Net::parse(p).unwrap()).collect(),
        hosts.iter().map(|h| (*h).to_owned()).collect(),
    );
    Rig {
        publisher,
        moonraker,
        _data: data,
        app: guarded(app, guard),
    }
}

impl Rig {
    fn publish(&self, policy: Policy) {
        self.publisher
            .set_access(AccessState::Known(Arc::new(policy)));
    }

    fn publish_unknown(&self, logins_required: bool) {
        self.publisher
            .set_access(AccessState::Unknown { logins_required });
    }

    /// Sends `method uri` from `peer` with `headers`, and returns the status and JSON body (or
    /// null for a body that is not JSON).
    async fn ask(
        &self,
        peer: &str,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let mut request = builder.body(Body::empty()).unwrap();
        let peer: std::net::IpAddr = peer.parse().unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(peer, 50000)));
        let (status, _, body) = send(&self.app, request).await;
        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    async fn get(&self, peer: &str, host: &str, uri: &str) -> (StatusCode, Value) {
        self.ask(peer, "GET", uri, &[("host", host)]).await
    }
}

fn code(body: &Value) -> &str {
    body["error"].as_str().unwrap_or_default()
}

const LAN: &str = "192.168.1.20";
const HOST: &str = "buster.local:7126";

#[tokio::test]
async fn a_trusted_client_reaches_the_ui_the_api_and_the_write_guard() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    for uri in ["/", "/api/status", "/api/sessions"] {
        let (status, _) = rig.get(LAN, HOST, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
    }
    let (status, body) = rig
        .ask(
            LAN,
            "DELETE",
            "/api/sessions/x",
            &[("host", HOST), ("x-wobblescope-client", "1")],
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(code(&body), "not_found");
}

#[tokio::test]
async fn a_start_from_a_trusted_client_passes_the_guard_and_meets_the_handler() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/tests")
        .header("host", HOST)
        .header("x-wobblescope-client", "1")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"chips": ["adxl345"], "axes": ["X"], "mode": "Pulse"}).to_string(),
        ))
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(LAN.parse().unwrap(), 50000)));
    let (status, _, body) = send(&rig.app, request).await;
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(code(&body), "moonraker_unavailable");
}

#[tokio::test]
async fn an_untrusted_client_is_refused_everywhere_before_any_handler() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    for (method, uri) in [
        ("GET", "/"),
        ("GET", "/api/sessions"),
        ("DELETE", "/api/sessions/x"),
        ("GET", "/api/live"),
    ] {
        let (status, body) = rig
            .ask(
                "203.0.113.9",
                method,
                uri,
                &[
                    ("host", HOST),
                    ("connection", "upgrade"),
                    ("upgrade", "websocket"),
                    ("sec-websocket-version", "13"),
                    ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
                ],
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
        assert_eq!(code(&body), "untrusted_client");
    }
}

#[tokio::test]
async fn a_rebinding_host_is_refused_with_or_without_a_matching_origin() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    let host = "rebind.example:7126";
    for (method, uri) in [("GET", "/"), ("DELETE", "/api/sessions/x")] {
        for origin in [None, Some("http://rebind.example:7126")] {
            let mut headers = vec![("host", host)];
            if let Some(origin) = origin {
                headers.push(("origin", origin));
            }
            let (status, body) = rig.ask(LAN, method, uri, &headers).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(code(&body), "host_not_allowed");
        }
    }
}

#[tokio::test]
async fn a_proxy_that_rewrites_host_fails_on_origin_with_a_message_that_says_what_to_change() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    let (status, body) = rig
        .ask(
            "127.0.0.1",
            "GET",
            "/api/status",
            &[
                ("host", "127.0.0.1:7126"),
                ("origin", "https://printer.example.com"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code(&body), "cross_origin");
    let message = body["message"].as_str().unwrap();
    assert!(message.contains("Host") && message.contains("cors_domains"));
}

#[tokio::test]
async fn host_values_that_name_an_address_or_the_machine_are_allowed() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    for host in [
        "192.168.1.5:7126",
        "[fe80::1]:7126",
        "localhost:7126",
        "BUSTER.local.",
    ] {
        let (status, _) = rig.get(LAN, host, "/api/status").await;
        assert_eq!(status, StatusCode::OK, "{host}");
    }
    let (status, body) = rig.ask(LAN, "GET", "/api/status", &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code(&body), "host_not_allowed");
}

#[tokio::test]
async fn origins_pass_when_they_are_the_daemons_own_or_listed_and_not_otherwise() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    for origin in ["http://buster.local:7126", "https://my.mainsail.xyz"] {
        let (status, _) = rig
            .ask(
                LAN,
                "GET",
                "/api/status",
                &[("host", HOST), ("origin", origin)],
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{origin}");
    }
    for origin in ["http://evil.example", "null"] {
        let (status, body) = rig
            .ask(
                LAN,
                "GET",
                "/api/status",
                &[("host", HOST), ("origin", origin)],
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origin}");
        assert_eq!(code(&body), "cross_origin");
    }
}

#[tokio::test]
async fn a_cross_site_write_from_a_trusted_client_still_meets_the_write_guard() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    let (status, body) = rig
        .ask(
            LAN,
            "DELETE",
            "/api/sessions/x",
            &[
                ("host", HOST),
                ("origin", "http://buster.local:7126"),
                ("sec-fetch-site", "cross-site"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code(&body), "cross_origin");
}

#[tokio::test]
async fn logins_required_refuses_everyone_including_loopback_and_names_force_logins() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy().with_logins_required(true));
    for peer in ["127.0.0.1", LAN] {
        for uri in ["/", "/api/status"] {
            let (status, body) = rig.get(peer, HOST, uri).await;
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(code(&body), "logins_required");
            assert!(body["message"].as_str().unwrap().contains("force_logins"));
        }
    }
    rig.publish(real_policy());
    let (status, _) = rig.get(LAN, HOST, "/api/status").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn the_logins_marker_refuses_loopback_too_and_its_absence_lets_loopback_in() {
    let rig = rig(&[], &[]);
    rig.publish_unknown(true);
    for peer in ["127.0.0.1", LAN] {
        let (status, body) = rig
            .ask(
                peer,
                "DELETE",
                "/api/sessions/x",
                &[("host", "localhost:7126"), ("x-wobblescope-client", "1")],
            )
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(code(&body), "logins_required");
    }
    rig.publish_unknown(false);
    let (status, body) = rig
        .ask(
            "127.0.0.1",
            "DELETE",
            "/api/sessions/x",
            &[("host", "localhost:7126"), ("x-wobblescope-client", "1")],
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}

#[tokio::test]
async fn before_the_lists_are_read_only_loopback_is_admitted_and_then_the_lan_follows() {
    let rig = rig(&[], &[]);
    for uri in ["/", "/api/status"] {
        let (status, _) = rig.get("127.0.0.1", "localhost:7126", uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
    }
    let (status, body) = rig.get(LAN, HOST, "/api/status").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code(&body), "access_unavailable");
    rig.publish(real_policy());
    let (status, _) = rig.get(LAN, HOST, "/api/status").await;
    assert_eq!(status, StatusCode::OK);
    assert!(matches!(rig.moonraker.access(), AccessState::Known(_)));
}

#[tokio::test]
async fn before_the_lists_are_read_loopback_still_meets_the_host_and_origin_checks() {
    let rig = rig(&[], &["buster.example"]);
    let (status, body) = rig.get("127.0.0.1", "buster.local", "/api/status").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code(&body), "host_not_allowed");
    let (status, _) = rig.get("127.0.0.1", "buster.example", "/api/status").await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = rig
        .ask(
            "127.0.0.1",
            "GET",
            "/api/status",
            &[
                ("host", "localhost:7126"),
                ("origin", "http://evil.example"),
            ],
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code(&body), "cross_origin");
}

#[tokio::test]
async fn forwarded_addresses_count_only_from_a_configured_proxy() {
    let rig = rig(&["127.0.0.1"], &[]);
    rig.publish(real_policy());
    let ask = |forwarded: &'static [&'static str]| {
        let rig = &rig;
        async move {
            let mut headers = vec![("host", HOST)];
            headers.extend(forwarded.iter().map(|v| ("x-forwarded-for", *v)));
            rig.ask("127.0.0.1", "GET", "/api/status", &headers).await
        }
    };
    assert_eq!(ask(&["192.168.1.20"]).await.0, StatusCode::OK);
    let (status, body) = ask(&["203.0.113.9"]).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "untrusted_client")
    );
    assert_eq!(ask(&["203.0.113.9, 192.168.1.20"]).await.0, StatusCode::OK);
    assert_eq!(
        ask(&["203.0.113.9", "192.168.1.20"]).await.0,
        StatusCode::OK
    );
    let (status, body) = ask(&["192.168.1.20", "203.0.113.9"]).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "untrusted_client")
    );
    let (status, body) = ask(&["banana"]).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "untrusted_client")
    );
    assert_eq!(ask(&["127.0.0.1"]).await.0, StatusCode::OK);
    let (status, body) = ask(&["203.0.113.9, 127.0.0.1"]).await;
    assert_eq!(
        (status, code(&body)),
        (StatusCode::FORBIDDEN, "untrusted_client")
    );
}

/// A log writer that keeps what it is given.
#[derive(Clone, Default)]
struct Captured(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Self;

    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

#[tokio::test]
async fn without_proxies_the_forwarded_header_is_ignored_and_warned_about_once() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    let log = Captured::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(log.clone())
        .with_max_level(tracing::Level::WARN)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    for _ in 0..2 {
        let (status, _) = rig
            .ask(
                "127.0.0.1",
                "GET",
                "/api/status",
                &[
                    ("host", "localhost:7126"),
                    ("x-forwarded-for", "203.0.113.9"),
                ],
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }
    let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert_eq!(text.matches("is ignored because").count(), 1, "{text}");
}

#[tokio::test]
async fn a_mapped_peer_is_judged_as_ipv4() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    let (status, _) = rig.get("::ffff:192.168.1.20", HOST, "/api/status").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = rig.get("::ffff:203.0.113.9", HOST, "/api/status").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_request_without_connect_info_is_refused_as_internal() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    let (status, _, body) = send(&rig.app, request("GET", "/api/status")).await;
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(code(&body), "internal");
}

#[tokio::test]
async fn an_unknown_ui_path_from_an_untrusted_peer_gets_the_json_refusal() {
    let rig = rig(&[], &[]);
    rig.publish(real_policy());
    let (status, body) = rig.get("203.0.113.9", HOST, "/sessions/abc").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(code(&body), "untrusted_client");
}

async fn serve(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await;
    });
    addr
}

#[tokio::test]
async fn the_live_socket_upgrades_for_a_trusted_peer_and_not_for_one_the_policy_excludes() {
    let rig = rig(&[], &[]);
    let trusting_loopback = Policy::build(
        &json!({"config": {"authorization": {
            "trusted_clients": ["127.0.0.0/8"], "cors_domains": []}}}),
        false,
        None,
    )
    .unwrap();
    rig.publish(trusting_loopback);
    let addr = serve(rig.app.clone()).await;
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/api/live"))
        .await
        .unwrap();
    let first = socket.next().await.unwrap().unwrap();
    assert!(first.is_text());

    // The real lists trust loopback too, so exclude it with a list that does not.
    let lan_only = Policy::build(
        &json!({"config": {"authorization": {
            "trusted_clients": ["192.168.0.0/16"], "cors_domains": []}}}),
        false,
        None,
    )
    .unwrap();
    rig.publish(lan_only);
    let refused = tokio_tungstenite::connect_async(format!("ws://{addr}/api/live")).await;
    match refused {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        other => panic!("expected a 403 on the upgrade, got {other:?}"),
    }
}
