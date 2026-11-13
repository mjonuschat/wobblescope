mod common;

use std::io;
use std::sync::Arc;

use axum::http::StatusCode;
use common::{Spec, app, json, open, probe, request, seal_in, send, sessions_dir, settings};
use sha2::Digest;
use tempfile::TempDir;
use wobblescope::storage::Storage;

const BIG: u64 = u64::MAX / 4;

fn ids(body: &serde_json::Value) -> Vec<&str> {
    body["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect()
}

fn three(data: &std::path::Path) {
    seal_in(data, &Spec::new("a", "2026-10-01T10:00:00Z"));
    seal_in(
        data,
        &Spec::new("b", "2026-10-02T10:00:00Z")
            .chips(&["adxl345 nh36"])
            .printer("voron"),
    );
    seal_in(data, &Spec::new("c", "2026-10-03T10:00:00Z"));
}

#[tokio::test]
async fn sessions_list_newest_first() {
    let dir = TempDir::new().unwrap();
    three(dir.path());
    let (s, _) = open(dir.path(), BIG);
    let (status, body) = json(&app(Some(s)), "GET", "/api/sessions").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), ["c", "b", "a"]);
}

#[tokio::test]
async fn a_creation_range_is_start_inclusive_and_end_exclusive() {
    let dir = TempDir::new().unwrap();
    three(dir.path());
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let (_, body) = json(
        &app,
        "GET",
        "/api/sessions?created_from=2026-10-02T00:00:00Z&created_to=2026-10-03T00:00:00Z",
    )
    .await;
    assert_eq!(ids(&body), ["b"]);
    let (_, body) = json(
        &app,
        "GET",
        "/api/sessions?created_from=2026-10-02T10:00:00Z&created_to=2026-10-03T10:00:00Z",
    )
    .await;
    assert_eq!(ids(&body), ["b"]);
}

#[tokio::test]
async fn range_bounds_compare_at_whole_seconds() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("f", "2026-10-31T12:00:00.100Z"));
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let (_, body) = json(
        &app,
        "GET",
        "/api/sessions?created_to=2026-10-31T12:00:00.500Z",
    )
    .await;
    assert!(ids(&body).is_empty());
    let (_, body) = json(
        &app,
        "GET",
        "/api/sessions?created_from=2026-10-31T12:00:00.900Z",
    )
    .await;
    assert_eq!(ids(&body), ["f"]);
}

#[tokio::test]
async fn chip_printer_and_pinned_filters_combine() {
    let dir = TempDir::new().unwrap();
    three(dir.path());
    let (s, _) = open(dir.path(), BIG);
    s.pin("a").unwrap();
    let app = app(Some(s));
    let (_, body) = json(
        &app,
        "GET",
        "/api/sessions?chip=adxl345%20nh36&pinned=false",
    )
    .await;
    assert_eq!(ids(&body), ["b"]);
    let (_, body) = json(&app, "GET", "/api/sessions?chip=adxl345%20nh36&pinned=true").await;
    assert!(ids(&body).is_empty());
    let (_, body) = json(&app, "GET", "/api/sessions?printer=voron").await;
    assert_eq!(ids(&body), ["b"]);
    let (_, body) = json(&app, "GET", "/api/sessions?pinned=true").await;
    assert_eq!(ids(&body), ["a"]);
}

#[tokio::test]
async fn a_bad_or_unknown_parameter_is_refused_by_name() {
    let dir = TempDir::new().unwrap();
    three(dir.path());
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    for (uri, name) in [
        ("/api/sessions?pinned=maybe", "pinned"),
        ("/api/sessions?created_from=yesterday", "created_from"),
        ("/api/sessions?created_to=soon", "created_to"),
        ("/api/sessions?colour=red", "colour"),
    ] {
        let (status, body) = json(&app, "GET", uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        assert_eq!(body["error"], "invalid_request");
        assert!(body["message"].as_str().unwrap().contains(name), "{body}");
    }
}

#[tokio::test]
async fn detail_returns_the_summary_and_the_stored_metadata() {
    let dir = TempDir::new().unwrap();
    let spec = Spec::new("b", "2026-10-02T10:00:00Z").chips(&["adxl345", "ebb36"]);
    seal_in(dir.path(), &spec);
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let (status, body) = json(&app, "GET", "/api/sessions/b").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["summary"]["id"], "b");
    assert_eq!(body["meta"], serde_json::to_value(spec.meta()).unwrap());
    let (status, body) = json(&app, "GET", "/api/sessions/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not_found");
}

#[tokio::test]
async fn samples_are_the_stored_file_with_a_content_etag() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("b", "2026-10-02T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    let (status, headers, body) =
        send(&app(Some(s)), request("GET", "/api/sessions/b/samples/0/0")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/octet-stream");
    assert_eq!(headers["cache-control"], "private, no-cache");
    let on_disk = std::fs::read(sessions_dir(dir.path()).join("b/c0-s0.zst")).unwrap();
    assert_eq!(body, on_disk);
    let expected_etag = format!(
        "\"{}\"",
        sha2::Sha256::digest(&body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    assert_eq!(headers["etag"], expected_etag);
}

#[tokio::test]
async fn a_matching_if_none_match_answers_304_without_a_body() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("b", "2026-10-02T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let (_, headers, _) = send(&app, request("GET", "/api/sessions/b/samples/0/0")).await;
    let etag = headers["etag"].to_str().unwrap();
    for test_tag in [
        etag.to_owned(),
        format!("W/{}", etag),
        format!("\"x\", {}", etag),
        "*".to_owned(),
    ] {
        let mut req = request("GET", "/api/sessions/b/samples/0/0");
        *req.headers_mut() = axum::http::HeaderMap::new();
        req.headers_mut()
            .insert("if-none-match", test_tag.parse().unwrap());
        let (status, headers, body) = send(&app, req).await;
        assert_eq!(status, StatusCode::NOT_MODIFIED, "tag: {}", test_tag);
        assert!(body.is_empty(), "tag: {}", test_tag);
        assert_eq!(headers["etag"], etag, "tag: {}", test_tag);
        assert_eq!(
            headers["cache-control"], "private, no-cache",
            "tag: {}",
            test_tag
        );
    }
}

#[tokio::test]
async fn a_stale_if_none_match_answers_200_with_the_body() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("b", "2026-10-02T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let mut req = request("GET", "/api/sessions/b/samples/0/0");
    req.headers_mut()
        .insert("if-none-match", "\"0000\"".parse().unwrap());
    let (status, _, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.is_empty());
}

#[tokio::test]
async fn a_reused_id_with_other_bytes_is_never_answered_304() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();
    seal_in(dir1.path(), &Spec::new("b", "2026-10-02T10:00:00Z"));
    seal_in(
        dir2.path(),
        &Spec::new("b", "2026-10-02T10:00:00Z").samples(300),
    );
    let (s1, _) = open(dir1.path(), BIG);
    let (s2, _) = open(dir2.path(), BIG);
    let app1 = app(Some(s1));
    let app2 = app(Some(s2));
    let (_, headers1, body1) = send(&app1, request("GET", "/api/sessions/b/samples/0/0")).await;
    let etag1 = headers1["etag"].to_str().unwrap();
    let mut req = request("GET", "/api/sessions/b/samples/0/0");
    req.headers_mut()
        .insert("if-none-match", etag1.parse().unwrap());
    let (status, headers2, body2) = send(&app2, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(body1, body2);
    assert_ne!(headers1["etag"], headers2["etag"]);
}

#[tokio::test]
async fn an_unrecorded_pair_is_not_found_and_a_malformed_one_is_a_bad_request() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("b", "2026-10-02T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let (status, body) = json(&app, "GET", "/api/sessions/b/samples/1/0").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not_found");
    let (status, body) = json(&app, "GET", "/api/sessions/b/samples/a/0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_request");
}

#[tokio::test]
async fn storage_use_reports_the_index_the_cap_and_the_probe() {
    let dir = TempDir::new().unwrap();
    three(dir.path());
    let (s, _) = open(dir.path(), BIG);
    let used = s.usage().unwrap().used_bytes;
    let sizes: u64 = ["a", "b", "c"]
        .iter()
        .map(|id| s.get(id).unwrap().size_bytes)
        .sum();
    let s = Storage::open(dir.path(), &settings(5_000_000, 0), probe(777))
        .unwrap()
        .0;
    let (status, body) = json(&app(Some(s)), "GET", "/api/storage").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["used_bytes"], sizes);
    assert_eq!(body["used_bytes"], used);
    assert_eq!(body["cap_bytes"], 5_000_000);
    assert_eq!(body["free_bytes"], 777);
    assert_eq!(body["session_count"], 3);
}

#[tokio::test]
async fn a_failing_probe_is_an_internal_error() {
    let dir = TempDir::new().unwrap();
    let failing: wobblescope::storage::Probe = Arc::new(|_| Err(io::Error::other("boom")));
    let s = Storage::open(dir.path(), &settings(BIG, 0), failing)
        .unwrap()
        .0;
    let (status, body) = json(&app(Some(s)), "GET", "/api/storage").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "internal");
}

#[tokio::test]
async fn unknown_api_paths_and_methods_answer_json() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    for uri in ["/api/nope", "/api/sessions/", "/api"] {
        let (status, headers, body) = send(&app, request("GET", uri)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert!(
            headers["content-type"]
                .to_str()
                .unwrap()
                .starts_with("application/json"),
            "{uri}"
        );
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["error"], "not_found");
    }
    let (status, body) = json(&app, "PUT", "/api/sessions").await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(body["error"], "method_not_allowed");
}

#[tokio::test]
async fn until_storage_opens_every_endpoint_answers_503() {
    let app = app(None);
    for uri in [
        "/api/sessions",
        "/api/sessions/a",
        "/api/sessions/a/samples/0/0",
        "/api/storage",
        "/api/sessions?colour=red",
    ] {
        let (status, body) = json(&app, "GET", uri).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert_eq!(body["error"], "storage_unavailable");
    }
    let (status, _) = json(&app, "GET", "/api/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
