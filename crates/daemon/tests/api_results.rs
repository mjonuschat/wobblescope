//! The saved-result routes.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{Spec, app, json, open, request, seal_in, send};
use serde_json::{Value, json};
use tempfile::TempDir;
use wobblescope::storage::Storage;

const CREATED: &str = "2026-10-01T10:00:00Z";
const BIG: u64 = u64::MAX / 4;

fn one_session() -> (TempDir, Storage) {
    let dir = TempDir::new().unwrap();
    seal_in(
        dir.path(),
        &Spec::new("s1", CREATED).chips(&["a", "b"]).samples(300),
    );
    let (s, _) = open(dir.path(), BIG);
    (dir, s)
}

fn save_body(session_created_at: &str, core_version: &str) -> Value {
    json!({
        "label": "before",
        "core_version": core_version,
        "session_created_at": session_created_at,
        "fit_params": wobblescope_core::fit::FitParams::default(),
        "overrides": [],
        "axes": [{
            "axis": "X",
            "analysis": {
                "per_chip": [],
                "worst_case": null,
                "worst_case_vibrations": [],
                "summed": null,
                "failures": [],
                "eligibility": { "decisions": [] },
                "core_version": core_version,
            },
            "snippets": { "worst_case": null, "per_chip": [] },
        }],
    })
}

fn post(uri: &str, headers: &[(&str, &str)], body: &Value) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json");
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    b.body(Body::from(body.to_string())).unwrap()
}

fn ui_post(uri: &str, body: &Value) -> Request<Body> {
    post(uri, &[("x-wobblescope-client", "1")], body)
}

fn ui_delete(uri: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(uri)
        .header("x-wobblescope-client", "1")
        .body(Body::empty())
        .unwrap()
}

fn parse(body: &[u8]) -> Value {
    serde_json::from_slice(body).unwrap()
}

async fn save(app: &axum::Router) -> String {
    let (status, _, body) = send(
        app,
        ui_post("/api/sessions/s1/results", &save_body(CREATED, "0.1.0")),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    parse(&body)["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn saves_and_lists_a_result() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    let id = save(&app).await;
    let (status, list) = json(&app, "GET", "/api/results").await;
    assert_eq!(status, StatusCode::OK);
    let results = list["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["id"], id);
    assert_eq!(results[0]["session_id"], "s1");
    let (status, detail) = json(&app, "GET", &format!("/api/results/{id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["record"]["label"], "before");
}

#[tokio::test]
async fn a_save_without_the_client_header_from_a_browser_is_refused() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    let req = post(
        "/api/sessions/s1/results",
        &[("origin", "http://evil.example")],
        &save_body(CREATED, "0.1.0"),
    );
    let (status, _, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(parse(&body)["error"], "cross_origin");
    let (_, list) = json(&app, "GET", "/api/results").await;
    assert!(list["results"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_stale_session_answers_409() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    let (status, _, body) = send(
        &app,
        ui_post(
            "/api/sessions/s1/results",
            &save_body("2026-10-02T10:00:00Z", "0.1.0"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(parse(&body)["error"], "session_changed");
}

#[tokio::test]
async fn a_missing_session_answers_404() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    let (status, _, body) = send(
        &app,
        ui_post("/api/sessions/nope/results", &save_body(CREATED, "0.1.0")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(parse(&body)["error"], "not_found");
}

#[tokio::test]
async fn downloads_a_result_sample_byte_for_byte() {
    let (d, s) = one_session();
    let app = app(Some(s));
    let id = save(&app).await;
    let (status, headers, body) = send(
        &app,
        request("GET", &format!("/api/results/{id}/samples/1/0")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/octet-stream");
    assert_eq!(headers["cache-control"], "no-store");
    let on_disk =
        std::fs::read(common::sessions_dir(d.path()).join("s1").join("c1-s0.zst")).unwrap();
    assert_eq!(body, on_disk);
    let (status, _, _) = send(
        &app,
        request("GET", &format!("/api/results/{id}/samples/9/0")),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_damaged_result_answers_500_result_unusable_and_serves_nothing() {
    let (d, s) = one_session();
    let app = app(Some(s));
    let id = save(&app).await;
    let path = d.path().join("results").join(format!("{id}.zip"));
    let mut bytes = std::fs::read(&path).unwrap();
    let sample =
        std::fs::read(common::sessions_dir(d.path()).join("s1").join("c1-s0.zst")).unwrap();
    let at = bytes
        .windows(sample.len())
        .position(|w| w == sample)
        .expect("the sample is stored uncompressed in the zip");
    bytes[at + sample.len() / 2] ^= 0xFF;
    std::fs::write(&path, bytes).unwrap();
    for uri in [
        format!("/api/results/{id}"),
        format!("/api/results/{id}/samples/1/0"),
    ] {
        let (status, _, body) = send(&app, request("GET", &uri)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{uri}");
        let body = parse(&body);
        assert_eq!(body["error"], "result_unusable", "{uri}");
        assert!(!body["message"].as_str().unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_core_version_mismatch_answers_400() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    let mut body = save_body(CREATED, "0.1.0");
    body["axes"][0]["analysis"]["core_version"] = json!("9.9.9");
    let (status, _, out) = send(&app, ui_post("/api/sessions/s1/results", &body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(parse(&out)["error"], "invalid_request");
}

#[tokio::test]
async fn deleting_a_result_answers_204_then_404() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    let id = save(&app).await;
    let uri = format!("/api/results/{id}");
    let (status, _, _) = send(&app, ui_delete(&uri)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, body) = send(&app, ui_delete(&uri)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(parse(&body)["error"], "not_found");
}

#[tokio::test]
async fn results_answer_503_before_storage_opens() {
    let app = app(None);
    for (method, uri) in [("GET", "/api/results"), ("GET", "/api/results/r1")] {
        let (status, body) = json(&app, method, uri).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert_eq!(body["error"], "storage_unavailable");
    }
    let (status, _, body) = send(
        &app,
        ui_post("/api/sessions/s1/results", &save_body(CREATED, "0.1.0")),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(parse(&body)["error"], "storage_unavailable");
}

#[tokio::test]
async fn an_oversized_save_body_answers_413_too_large() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    let mut body = save_body(CREATED, "0.1.0");
    body["label"] = json!("x".repeat(16 * 1024 * 1024 + 1));
    let (status, _, out) = send(&app, ui_post("/api/sessions/s1/results", &body)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(parse(&out)["error"], "too_large");
}

#[tokio::test]
async fn a_save_body_with_an_unknown_field_answers_400() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    let mut body = save_body(CREATED, "0.1.0");
    body["extra"] = json!(1);
    let (status, _, out) = send(&app, ui_post("/api/sessions/s1/results", &body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(parse(&out)["error"], "invalid_request");
}

#[tokio::test]
async fn a_result_id_with_path_traversal_is_refused() {
    let (d, s) = one_session();
    let app = app(Some(s));
    let id = save(&app).await;
    let outside = d.path().join("victim.zip");
    std::fs::write(&outside, b"keep").unwrap();
    for rid in ["..", "%2e%2e", "..%2fvictim", "%2e%2e%2fvictim"] {
        for method in ["GET", "DELETE"] {
            let req = if method == "GET" {
                request("GET", &format!("/api/results/{rid}"))
            } else {
                ui_delete(&format!("/api/results/{rid}"))
            };
            let (status, _, body) = send(&app, req).await;
            assert!(status.is_client_error(), "{method} {rid}: {status}");
            let body = parse(&body);
            assert!(
                body["error"].is_string() && body["message"].is_string(),
                "{method} {rid}"
            );
        }
    }
    assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
    let (status, _) = json(&app, "GET", &format!("/api/results/{id}")).await;
    assert_eq!(status, StatusCode::OK);
}
