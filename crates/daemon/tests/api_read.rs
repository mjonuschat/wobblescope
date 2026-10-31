mod common;

use std::io;
use std::sync::Arc;

use axum::http::StatusCode;
use common::{Spec, app, json, open, probe, request, seal_in, send, sessions_dir, settings};
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
async fn samples_are_the_stored_file_unchanged_and_uncached() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("b", "2026-10-02T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    let (status, headers, body) =
        send(&app(Some(s)), request("GET", "/api/sessions/b/samples/0/0")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/octet-stream");
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(
        body,
        std::fs::read(sessions_dir(dir.path()).join("b/c0-s0.zst")).unwrap()
    );
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
async fn without_storage_every_endpoint_answers_503() {
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
        assert_eq!(body["error"], "storage_disabled");
    }
    let (status, _) = json(&app, "GET", "/api/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
