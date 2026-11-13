mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{Spec, app, json, open, request, seal_in, send};
use serde_json::{Value, json};
use sha2::Digest;
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
    assert_eq!(headers["cache-control"], "private, no-cache");
    let on_disk =
        std::fs::read(common::sessions_dir(d.path()).join("s1").join("c1-s0.zst")).unwrap();
    assert_eq!(body, on_disk);
    let expected_etag = format!(
        "\"{}\"",
        sha2::Sha256::digest(&body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    assert_eq!(headers["etag"], expected_etag);
    let mut req = request("GET", &format!("/api/results/{id}/samples/1/0"));
    req.headers_mut()
        .insert("if-none-match", expected_etag.parse().unwrap());
    let (status, headers, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::NOT_MODIFIED);
    assert!(body.is_empty());
    assert_eq!(headers["etag"], expected_etag);
    assert_eq!(headers["cache-control"], "private, no-cache");
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

#[tokio::test]
async fn a_session_downloads_as_a_zip_of_its_meta_and_samples() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    let (status, headers, body) = send(&app, get_request("/api/sessions/s1/download")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/zip");
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .contains("s1.zip")
    );
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(body)).unwrap();
    let names: Vec<String> = archive
        .file_names()
        .map(|n| n.unwrap().into_owned())
        .collect();
    assert!(names.contains(&"meta.json".to_owned()));
    assert!(names.iter().any(|n| n.starts_with("samples/c0-s0")));
    let mut meta = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("meta.json").unwrap(), &mut meta).unwrap();
    assert!(serde_json::from_str::<Value>(&meta).is_ok());

    let (status, _, _) = send(&app, get_request("/api/sessions/nope/download")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

fn get_request(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn a_second_session_download_is_refused_while_the_first_is_unread() {
    let (_d, s) = one_session();
    let (_first, guard) = s.session_zip("s1").unwrap();
    assert!(matches!(
        s.session_zip("s1"),
        Err(wobblescope::storage::StorageError::DownloadBusy)
    ));
    drop(guard);
    assert!(s.session_zip("s1").is_ok());
}

fn install_legacy(dir: &std::path::Path, label: &str) -> String {
    let from = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/data/results/legacy_{label}.zip"));
    let id = format!("legacy-{label}");
    std::fs::copy(from, dir.join("results").join(format!("{id}.zip"))).unwrap();
    id
}

/// Keys of `Analysis` that results saved by older versions do not have.
const ADDED: [&str; 4] = [
    "damping_source",
    "resonance_freq",
    "for_performance",
    "peaks",
];

fn without_added_keys(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for key in ADDED {
                map.remove(key);
            }
            map.values_mut().for_each(without_added_keys);
        }
        Value::Array(items) => items.iter_mut().for_each(without_added_keys),
        _ => {}
    }
}

fn assert_same(label: &str, got: &Value, want: &Value) {
    match (got, want) {
        (Value::Number(g), Value::Number(w)) => {
            let (g, w) = (g.as_f64().unwrap(), w.as_f64().unwrap());
            assert!(
                (g - w).abs() <= 1e-9 * w.abs().max(1e-300) + 1e-12,
                "{label}: {g} vs {w}"
            );
        }
        (Value::Object(g), Value::Object(w)) => {
            assert_eq!(
                g.keys().collect::<Vec<_>>(),
                w.keys().collect::<Vec<_>>(),
                "{label}: keys"
            );
            for (key, g) in g {
                assert_same(&format!("{label}.{key}"), g, &w[key]);
            }
        }
        (Value::Array(g), Value::Array(w)) => {
            assert_eq!(g.len(), w.len(), "{label}: length");
            for (i, (g, w)) in g.iter().zip(w).enumerate() {
                assert_same(&format!("{label}[{i}]"), g, w);
            }
        }
        _ => assert_eq!(got, want, "{label}"),
    }
}

async fn open_legacy(label: &str) {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let id = install_legacy(dir.path(), label);
    let app = app(Some(s));
    let (status, detail) = json(&app, "GET", &format!("/api/results/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["record"]["version"], 1);
    // A missing or zero damping ratio meant 0.1 when this was saved. It must not become "estimate".
    assert_eq!(
        detail["record"]["fit_params"]["damping_ratio"], 0.1,
        "{label}"
    );

    let (status, _, bytes) = send(
        &app,
        request("GET", &format!("/api/results/{id}/samples/0/0")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let meta: wobblescope_session::meta::SessionMeta =
        serde_json::from_value(detail["meta"].clone()).unwrap();
    let params: wobblescope_core::fit::FitParams =
        serde_json::from_value(detail["record"]["fit_params"].clone()).unwrap();
    let inputs =
        wobblescope_session::bridge::dataset_inputs(&meta, wobblescope_core::fit::Axis::X, |_| {
            wobblescope_session::codec::decode_samples(&bytes)
        })
        .unwrap();
    let fresh = wobblescope_core::multichip::analyze(&inputs, &params).unwrap();
    let mut fresh = serde_json::to_value(fresh).unwrap();
    let mut saved = detail["record"]["axes"][0]["analysis"].clone();
    without_added_keys(&mut fresh);
    without_added_keys(&mut saved);
    assert_same("analysis", &fresh, &saved);
}

#[tokio::test]
async fn a_legacy_result_with_a_null_damping_ratio_opens_and_reanalyses_to_its_saved_answer() {
    open_legacy("null").await;
}

#[tokio::test]
async fn a_legacy_result_with_a_zero_damping_ratio_opens_and_reanalyses_to_its_saved_answer() {
    open_legacy("zero").await;
}

#[tokio::test]
async fn a_legacy_result_with_a_damping_ratio_opens_and_reanalyses_to_its_saved_answer() {
    open_legacy("number").await;
}
