mod common;

use std::path::Path;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{Spec, app, json, open, probe, request, seal_in, send, sessions_dir, settings};
use tempfile::TempDir;
use wobblescope::api::{ImportGate, mount_with};
use wobblescope::storage::Storage;

const BIG: u64 = u64::MAX / 4;
const CSV: &str = "#time,accel_x,accel_y,accel_z\n\
                   100.000000,1.0,2.0,3.0\n\
                   100.000313,1.5,2.5,3.5\n\
                   100.000626,2.0,3.0,4.0\n";

/// A write request. Unless a test says otherwise it carries the UI's client header.
fn write(method: &str, uri: &str, headers: &[(&str, &str)], body: Vec<u8>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri);
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    b.body(Body::from(body)).unwrap()
}

fn ui(method: &str, uri: &str) -> Request<Body> {
    write(method, uri, &[("x-wobblescope-client", "1")], vec![])
}

fn patch(uri: &str, content_type: &str, body: &str) -> Request<Body> {
    write(
        "PATCH",
        uri,
        &[
            ("x-wobblescope-client", "1"),
            ("content-type", content_type),
        ],
        body.as_bytes().to_vec(),
    )
}

fn import_req(query: &str, body: Vec<u8>) -> Request<Body> {
    write(
        "POST",
        &format!("/api/sessions?{query}"),
        &[("x-wobblescope-client", "1")],
        body,
    )
}

fn body_json(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).unwrap()
}

fn ids(s: &Storage) -> Vec<String> {
    let mut v: Vec<String> = s
        .list(&Default::default())
        .into_iter()
        .map(|x| x.id)
        .collect();
    v.sort();
    v
}

fn dirs(data: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(sessions_dir(data))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn one_session() -> (TempDir, Storage) {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    (dir, s)
}

/// Four equal sessions and a storage whose cap they exactly fill.
fn full(data: &Path) -> Storage {
    for (i, day) in ["01", "02", "03", "04"].iter().enumerate() {
        seal_in(
            data,
            &Spec::new(&format!("s{}", i + 1), &format!("2026-10-{day}T10:00:00Z")),
        );
    }
    let total = open(data, BIG).0.usage().unwrap().used_bytes;
    open(data, total).0
}

#[tokio::test]
async fn patch_pins_and_unpins() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    let (status, _, body) = send(
        &app,
        patch(
            "/api/sessions/s1",
            "application/json",
            r#"{"pinned": true}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body_json(&body)["pinned"], true);
    let (_, got) = json(&app, "GET", "/api/sessions/s1").await;
    assert_eq!(got["summary"]["pinned"], true);
    let (status, _, body) = send(
        &app,
        patch(
            "/api/sessions/s1",
            "application/json",
            r#"{"pinned": false}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body_json(&body)["pinned"], false);
}

#[tokio::test]
async fn a_bad_patch_body_is_a_bad_request() {
    let (_d, s) = one_session();
    let app = app(Some(s));
    for (ct, body) in [
        ("application/json", "{}"),
        ("application/json", r#"{"pinned": "yes"}"#),
        ("application/json", r#"{"pinned": true, "x": 1}"#),
        ("application/json", "not json"),
        ("text/plain", r#"{"pinned": true}"#),
    ] {
        let (status, _, out) = send(&app, patch("/api/sessions/s1", ct, body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{ct} {body}");
        assert_eq!(body_json(&out)["error"], "invalid_request");
    }
}

#[tokio::test]
async fn patch_of_an_unknown_id_is_not_found() {
    let (_d, s) = one_session();
    let (status, _, body) = send(
        &app(Some(s)),
        patch(
            "/api/sessions/nope",
            "application/json",
            r#"{"pinned": true}"#,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body_json(&body)["error"], "not_found");
}

#[tokio::test]
async fn delete_removes_an_unpinned_session_from_the_list_and_the_disk() {
    let (d, s) = one_session();
    let app = app(Some(s.clone()));
    let (status, _, body) = send(&app, ui("DELETE", "/api/sessions/s1")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(body.is_empty());
    assert!(ids(&s).is_empty());
    assert!(dirs(d.path()).is_empty());
}

#[tokio::test]
async fn delete_of_a_pinned_session_is_a_conflict_and_keeps_the_files() {
    let (d, s) = one_session();
    s.pin("s1").unwrap();
    let (status, _, body) = send(&app(Some(s)), ui("DELETE", "/api/sessions/s1")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body_json(&body)["error"], "pinned");
    assert_eq!(dirs(d.path()), ["s1"]);
}

#[tokio::test]
async fn delete_during_a_download_is_refused_until_it_ends() {
    let (d, s) = one_session();
    let app = app(Some(s.clone()));
    let guard = s.open_sample("s1", 0, 0).unwrap();
    let (status, _, body) = send(&app, ui("DELETE", "/api/sessions/s1")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body_json(&body)["error"], "in_use");
    assert_eq!(dirs(d.path()), ["s1"]);
    drop(guard);
    let (status, _, _) = send(&app, ui("DELETE", "/api/sessions/s1")).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_of_an_unknown_id_is_not_found() {
    let (_d, s) = one_session();
    let (status, _, _) = send(&app(Some(s)), ui("DELETE", "/api/sessions/nope")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_posted_csv_becomes_a_listed_import_session() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s.clone()));
    let (status, _, body) = send(
        &app,
        import_req(
            "chip=adxl345&axis=x&file_name=a.csv",
            CSV.as_bytes().to_vec(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let v = body_json(&body);
    assert_eq!(v["source"], "CsvImport");
    assert_eq!(v["chips"], serde_json::json!(["adxl345"]));
    assert_eq!(ids(&s), [v["id"].as_str().unwrap()]);
}

#[tokio::test]
async fn axis_and_point_may_come_from_the_file_name() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s.clone()));
    let name = "raw_data_y_adxl345_100.000_100.000_20.000_t.csv";
    let (status, _, body) = send(
        &app,
        import_req(
            &format!("chip=adxl345&file_name={name}"),
            CSV.as_bytes().to_vec(),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = body_json(&body)["id"].as_str().unwrap().to_owned();
    let (_, detail) = json(&app, "GET", &format!("/api/sessions/{id}")).await;
    assert_eq!(detail["meta"]["segments"][0]["axis"], "Y");
    assert_eq!(
        detail["meta"]["probe_points"][0]["position"],
        serde_json::json!([100.0, 100.0, 20.0])
    );
}

#[tokio::test]
async fn bad_import_parameters_are_refused_by_name() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    for (query, name) in [
        ("axis=x", "chip"),
        ("chip=", "chip"),
        ("chip=a&axis=z", "axis"),
        ("chip=a&axis=x&point=1,2", "point"),
        ("chip=a&axis=x&point=1,2,nan", "point"),
        ("chip=a", "axis"),
        ("chip=a&axis=x&colour=red", "colour"),
    ] {
        let (status, _, body) = send(&app, import_req(query, CSV.as_bytes().to_vec())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        let v = body_json(&body);
        assert_eq!(v["error"], "invalid_request");
        assert!(
            v["message"].as_str().unwrap().contains(name),
            "{query}: {v}"
        );
    }
}

#[tokio::test]
async fn a_malformed_csv_is_422_and_deletes_nothing() {
    let dir = TempDir::new().unwrap();
    let s = full(dir.path());
    let bad = "#t\n1.0,1,2,3\n2.0,1,2,3\n3.0,1\n";
    let (status, _, body) = send(
        &app(Some(s.clone())),
        import_req("chip=a&axis=x", bad.into()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let v = body_json(&body);
    assert_eq!(v["error"], "invalid_csv");
    assert!(v["message"].as_str().unwrap().contains("line"), "{v}");
    assert_eq!(ids(&s), ["s1", "s2", "s3", "s4"]);
}

#[tokio::test]
async fn a_full_cap_of_pinned_sessions_refuses_an_import_with_byte_counts() {
    let dir = TempDir::new().unwrap();
    let s = full(dir.path());
    for id in ["s1", "s2", "s3", "s4"] {
        s.pin(id).unwrap();
    }
    let total = s.usage().unwrap().used_bytes;
    let (status, _, body) = send(
        &app(Some(s)),
        import_req("chip=a&axis=x", CSV.as_bytes().to_vec()),
    )
    .await;
    assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE);
    let v = body_json(&body);
    assert_eq!(v["error"], "cap_exceeded");
    assert_eq!(v["need"], 144);
    assert_eq!(v["cap"], total);
    assert_eq!(v["pinned_bytes"], total);
    assert_eq!(v["evictable_bytes"], 0);
    assert_eq!(dirs(dir.path()), ["s1", "s2", "s3", "s4"]);
}

#[tokio::test]
async fn too_little_free_disk_refuses_an_import_with_byte_counts() {
    let dir = TempDir::new().unwrap();
    let s = Storage::open(dir.path(), &settings(BIG, 0), probe(10))
        .unwrap()
        .0;
    let (status, _, body) = send(
        &app(Some(s)),
        import_req("chip=a&axis=x", CSV.as_bytes().to_vec()),
    )
    .await;
    assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE);
    let v = body_json(&body);
    assert_eq!(v["error"], "insufficient_disk");
    assert_eq!(
        (v["need"].as_u64(), v["available"].as_u64()),
        (Some(144), Some(10))
    );
    assert!(dirs(dir.path()).is_empty());
}

#[tokio::test]
async fn bad_options_are_refused_before_the_permit_or_the_body_is_touched() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let gate = ImportGate::default();
    let app = mount_with(Some(s), gate.clone());
    // While another import holds the gate, a request with bad options still gets its 400.
    let _held = gate.try_acquire().unwrap();
    let polled = Arc::new(AtomicBool::new(false));
    let flag = polled.clone();
    let stream = futures_util::stream::once(async move {
        flag.store(true, Ordering::SeqCst);
        Ok::<_, std::io::Error>(axum::body::Bytes::from_static(b"#"))
    });
    let req = Request::builder()
        .method("POST")
        .uri("/api/sessions?axis=x")
        .header("x-wobblescope-client", "1")
        .body(Body::from_stream(stream))
        .unwrap();
    let (status, _, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body_json(&body)["error"], "invalid_request");
    assert!(!polled.load(Ordering::SeqCst), "the body was read");
}

#[tokio::test]
async fn bad_options_beat_an_oversized_body() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let big = vec![b'#'; 64 * 1024 * 1024 + 1];
    let (status, _, body) = send(&app(Some(s)), import_req("axis=x", big)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body_json(&body)["error"], "invalid_request");
}

#[tokio::test]
async fn a_body_over_64_mib_is_413() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let big = vec![b'#'; 64 * 1024 * 1024 + 1];
    let (status, _, body) = send(&app(Some(s)), import_req("chip=a&axis=x", big)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body_json(&body)["error"], "too_large");
}

#[tokio::test]
async fn a_second_import_while_one_runs_is_busy() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let gate = ImportGate::default();
    let app = mount_with(Some(s), gate.clone());
    let permit = gate.try_acquire().unwrap();
    let (status, _, body) = send(&app, import_req("chip=a&axis=x", CSV.as_bytes().to_vec())).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body_json(&body)["error"], "busy");
    assert!(dirs(dir.path()).is_empty());
    drop(permit);
    let (status, _, _) = send(&app, import_req("chip=a&axis=x", CSV.as_bytes().to_vec())).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn a_cross_site_delete_is_refused_and_changes_nothing() {
    let (_d, s) = one_session();
    let app = app(Some(s.clone()));
    let req = write(
        "DELETE",
        "/api/sessions/s1",
        &[("sec-fetch-site", "cross-site")],
        vec![],
    );
    let (status, _, body) = send(&app, req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body_json(&body)["error"], "cross_origin");
    assert_eq!(ids(&s), ["s1"]);
}

#[tokio::test]
async fn same_site_is_refused_even_with_the_client_header() {
    let (_d, s) = one_session();
    let req = write(
        "POST",
        "/api/sessions?chip=a&axis=x",
        &[
            ("sec-fetch-site", "same-site"),
            ("x-wobblescope-client", "1"),
        ],
        CSV.into(),
    );
    let (status, _, _) = send(&app(Some(s)), req).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_foreign_origin_without_the_client_header_is_refused() {
    let (d, s) = one_session();
    let app = app(Some(s));
    let evil = write(
        "POST",
        "/api/sessions?chip=a&axis=x",
        &[
            ("origin", "http://evil.example"),
            ("content-type", "text/plain"),
        ],
        CSV.into(),
    );
    let (status, _, _) = send(&app, evil).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(dirs(d.path()), ["s1"]);
    for origin in ["http://127.0.0.1:7126", "null"] {
        let req = write(
            "PATCH",
            "/api/sessions/s1",
            &[("origin", origin), ("content-type", "application/json")],
            br#"{"pinned": true}"#.to_vec(),
        );
        let (status, _, _) = send(&app, req).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origin}");
    }
}

#[tokio::test]
async fn a_write_with_only_origin_and_a_rewritten_host_is_accepted() {
    let (_d, s) = one_session();
    let req = write(
        "DELETE",
        "/api/sessions/s1",
        &[
            ("x-wobblescope-client", "1"),
            ("origin", "http://printer.local"),
            ("host", "127.0.0.1:7126"),
        ],
        vec![],
    );
    let (status, _, _) = send(&app(Some(s)), req).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn same_origin_and_origin_less_writes_pass() {
    let dir = TempDir::new().unwrap();
    seal_in(dir.path(), &Spec::new("s1", "2026-10-01T10:00:00Z"));
    seal_in(dir.path(), &Spec::new("s2", "2026-10-02T10:00:00Z"));
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let same = write(
        "DELETE",
        "/api/sessions/s1",
        &[
            ("x-wobblescope-client", "1"),
            ("sec-fetch-site", "same-origin"),
        ],
        vec![],
    );
    assert_eq!(send(&app, same).await.0, StatusCode::NO_CONTENT);
    let curl = write("DELETE", "/api/sessions/s2", &[], vec![]);
    assert_eq!(send(&app, curl).await.0, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn reads_are_never_refused_for_their_origin() {
    let (_d, s) = one_session();
    let mut req = request("GET", "/api/sessions");
    req.headers_mut()
        .insert("sec-fetch-site", "cross-site".parse().unwrap());
    assert_eq!(send(&app(Some(s)), req).await.0, StatusCode::OK);
}

#[tokio::test]
async fn without_storage_a_cross_site_write_is_403_and_a_same_origin_one_503() {
    let app = app(None);
    let cross = write(
        "DELETE",
        "/api/sessions/s1",
        &[("sec-fetch-site", "cross-site")],
        vec![],
    );
    assert_eq!(send(&app, cross).await.0, StatusCode::FORBIDDEN);
    let (status, _, body) = send(&app, ui("DELETE", "/api/sessions/s1")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(&body)["error"], "storage_disabled");
}
