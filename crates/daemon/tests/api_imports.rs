mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{app, json, open, send};
use serde_json::{Value, json};
use tempfile::TempDir;
use wobblescope::api::{ImportGate, mount_with};

const BIG: u64 = u64::MAX / 4;
const BOUNDARY: &str = "wobblescope-test-boundary";
const CSV: &str = "#time,accel_x,accel_y,accel_z\n\
                   100.000000,1.0,2.0,3.0\n\
                   100.000313,1.5,2.5,3.5\n\
                   100.000626,2.0,3.0,4.0\n";
const LATER_CSV: &str = "#time,accel_x,accel_y,accel_z\n\
                         200.000000,1.0,2.0,3.0\n\
                         200.000313,1.5,2.5,3.5\n\
                         200.000626,2.0,3.0,4.0\n";

fn multipart_body(session_json: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"session\"\r\n\
             Content-Type: application/json\r\n\r\n{session_json}\r\n"
        )
        .as_bytes(),
    );
    for (name, bytes) in files {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; \
                 filename=\"{name}\"\r\nContent-Type: text/csv\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

fn request_with(body: Vec<u8>, client_header: bool) -> Request<Body> {
    let mut b = Request::builder()
        .method("POST")
        .uri("/api/imports")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        );
    b = if client_header {
        b.header("x-wobblescope-client", "1")
    } else {
        b.header("origin", "http://evil.example")
            .header("sec-fetch-site", "cross-site")
    };
    b.body(Body::from(body)).unwrap()
}

fn multipart(session_json: &str, files: &[(&str, &[u8])]) -> Request<Body> {
    request_with(multipart_body(session_json, files), true)
}

fn body_json(body: &[u8]) -> Value {
    serde_json::from_slice(body).unwrap()
}

fn csv(chip: &str, axis: &str) -> Value {
    json!({"chip": chip, "axis": axis})
}

fn session(files: &[Value]) -> String {
    json!({ "files": files }).to_string()
}

#[tokio::test]
async fn one_file_makes_a_session_listed_as_a_csv_import() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s.clone()));
    let (status, _, body) = send(
        &app,
        multipart(
            &session(&[csv("adxl345", "X")]),
            &[("a.csv", CSV.as_bytes())],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let v = body_json(&body);
    assert_eq!(v["source"], "CsvImport");
    assert_eq!(v["chips"], json!(["adxl345"]));
    let (_, listed) = json(&app, "GET", "/api/sessions").await;
    assert_eq!(listed["sessions"][0]["id"], v["id"]);
}

#[tokio::test]
async fn two_chips_on_x_make_one_session_with_both_chips() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s.clone()));
    let (status, _, body) = send(
        &app,
        multipart(
            &session(&[csv("adxl345", "X"), csv("adxl345 bed", "x")]),
            &[("a.csv", CSV.as_bytes()), ("b.csv", CSV.as_bytes())],
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let v = body_json(&body);
    assert_eq!(v["chips"].as_array().unwrap().len(), 2);
    assert_eq!(s.list(&Default::default()).len(), 1);
}

#[tokio::test]
async fn x_and_y_files_make_two_segments() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let (status, _, body) = send(
        &app,
        multipart(
            &session(&[csv("adxl345", "X"), csv("adxl345", "Y")]),
            &[("x.csv", CSV.as_bytes()), ("y.csv", CSV.as_bytes())],
        ),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&body)
    );
    let id = body_json(&body)["id"].as_str().unwrap().to_owned();
    let (_, detail) = json(&app, "GET", &format!("/api/sessions/{id}")).await;
    assert_eq!(detail["meta"]["segments"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn a_part_count_that_does_not_match_the_json_is_refused() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s.clone()));
    let two = session(&[csv("a", "X"), csv("b", "X")]);
    for files in [
        vec![("a.csv", CSV.as_bytes())],
        vec![
            ("a.csv", CSV.as_bytes()),
            ("b.csv", CSV.as_bytes()),
            ("c.csv", CSV.as_bytes()),
        ],
    ] {
        let (status, _, body) = send(&app, multipart(&two, &files)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body_json(&body)["error"], "invalid_request");
    }
    assert!(s.list(&Default::default()).is_empty());
}

#[tokio::test]
async fn a_file_part_before_the_session_part_is_refused() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.csv\"\r\n\r\n\
         {CSV}\r\n--{BOUNDARY}--\r\n"
    );
    let (status, _, body) = send(&app, request_with(body.into_bytes(), true)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body_json(&body)["message"]
            .as_str()
            .unwrap()
            .contains("session")
    );
}

#[tokio::test]
async fn a_missing_chip_or_axis_is_refused_naming_the_file() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    for (second, word) in [
        (json!({"axis": "X"}), "chip"),
        (json!({"chip": ""}), "chip"),
        (json!({"chip": "b"}), "axis"),
        (json!({"chip": "b", "axis": "z"}), "axis"),
    ] {
        let (status, _, body) = send(
            &app,
            multipart(
                &session(&[csv("a", "X"), second]),
                &[("a.csv", CSV.as_bytes()), ("b.csv", CSV.as_bytes())],
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let message = body_json(&body)["message"].as_str().unwrap().to_owned();
        assert!(message.contains(word), "{message}");
        assert!(message.contains("file 2"), "{message}");
    }
}

#[tokio::test]
async fn a_bad_point_is_refused() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    for point in [json!([1.0, 2.0]), json!([1.0, 2.0, "x"]), json!("1,2,3")] {
        let (status, _, body) = send(
            &app,
            multipart(
                &session(&[json!({"chip": "a", "axis": "X", "point": point})]),
                &[("a.csv", CSV.as_bytes())],
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{point}");
        assert_eq!(body_json(&body)["error"], "invalid_request");
    }
}

#[tokio::test]
async fn a_bad_csv_names_its_position_and_file() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let bad = "#t\n1.0,1,2,3\n2.0,1,2,3\n3.0,1\n";
    let (status, _, body) = send(
        &app,
        multipart(
            &session(&[csv("a", "X"), csv("b", "X")]),
            &[("good.csv", CSV.as_bytes()), ("broken.csv", bad.as_bytes())],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let v = body_json(&body);
    assert_eq!(v["error"], "invalid_csv");
    assert_eq!(v["file"], 2);
    let message = v["message"].as_str().unwrap();
    assert!(message.contains("file 2 ("), "{message}");
    assert!(message.contains("broken.csv"), "{message}");

    let (status, _, body) = send(
        &app,
        single_file_request("chip=a&axis=x", bad.as_bytes().to_vec()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body_json(&body).get("file").is_none());
}

fn single_file_request(query: &str, body: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/api/sessions?{query}"))
        .header("x-wobblescope-client", "1")
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn a_duplicate_chip_axis_and_point_is_refused() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s.clone()));
    let (status, _, body) = send(
        &app,
        multipart(
            &session(&[csv("a", "X"), csv("a", "X")]),
            &[("a.csv", CSV.as_bytes()), ("b.csv", CSV.as_bytes())],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body_json(&body)["error"], "invalid_request");
    assert!(s.list(&Default::default()).is_empty());
}

#[tokio::test]
async fn files_that_do_not_overlap_in_time_are_refused_as_not_the_same_run() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s.clone()));
    let (status, _, body) = send(
        &app,
        multipart(
            &session(&[csv("a", "X"), csv("b", "X")]),
            &[("a.csv", CSV.as_bytes()), ("b.csv", LATER_CSV.as_bytes())],
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body_json(&body)["error"], "invalid_request");
    assert!(s.list(&Default::default()).is_empty());
}

#[tokio::test]
async fn created_at_is_normalized_to_utc() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let text = json!({
        "created_at": "2026-11-12T10:00:00.789+02:00",
        "files": [csv("a", "X")],
    })
    .to_string();
    let (status, _, body) = send(&app, multipart(&text, &[("a.csv", CSV.as_bytes())])).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body_json(&body)["created_at"], "2026-11-12T08:00:00Z");
}

#[tokio::test]
async fn an_unparseable_created_at_is_refused() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let text = json!({"created_at": "yesterday", "files": [csv("a", "X")]}).to_string();
    let (status, _, body) = send(&app, multipart(&text, &[("a.csv", CSV.as_bytes())])).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body_json(&body)["message"]
            .as_str()
            .unwrap()
            .contains("created_at")
    );
}

#[tokio::test]
async fn seventeen_files_are_refused() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s.clone()));
    let specs: Vec<Value> = (0..17).map(|i| csv(&format!("c{i}"), "X")).collect();
    let files: Vec<(&str, &[u8])> = (0..17).map(|_| ("a.csv", CSV.as_bytes())).collect();
    let (status, _, body) = send(&app, multipart(&session(&specs), &files)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body_json(&body)["error"], "invalid_request");
    assert!(s.list(&Default::default()).is_empty());
}

#[tokio::test]
async fn a_session_part_over_64_kib_is_refused_with_413() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s.clone()));
    let entries = vec!["{}"; 20_000].join(",");
    let json = format!("{{\"files\":[{entries}]}}");
    let padded = format!(
        "{json}{}",
        " ".repeat(64 * 1024 + 1 - json.len().min(64 * 1024))
    );
    assert!(padded.len() > 64 * 1024);
    let (status, _, body) = send(&app, multipart(&padded, &[])).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body_json(&body)["error"], "too_large");
    assert!(s.list(&Default::default()).is_empty());
}

#[tokio::test]
async fn file_parts_over_64_mib_in_total_are_refused_with_413() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s.clone()));
    let big = vec![0u8; 64 * 1024 * 1024 + 1];
    let (status, _, body) = send(
        &app,
        multipart(&session(&[csv("a", "X")]), &[("big.csv", &big)]),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body_json(&body)["error"], "too_large");
    assert!(s.list(&Default::default()).is_empty());
}

#[tokio::test]
async fn a_second_import_while_one_runs_is_busy() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let gate = ImportGate::default();
    let app = mount_with(
        common::cell(Some(s)),
        common::idle_moonraker(),
        gate.clone(),
    );
    let permit = gate.try_acquire().unwrap();
    let (status, _, body) = send(
        &app,
        multipart(&session(&[csv("a", "X")]), &[("a.csv", CSV.as_bytes())]),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body_json(&body)["error"], "busy");
    drop(permit);
}

#[tokio::test]
async fn a_request_without_the_ui_header_is_refused_as_cross_origin_like_other_writes() {
    let dir = TempDir::new().unwrap();
    let (s, _) = open(dir.path(), BIG);
    let app = app(Some(s));
    let body = multipart_body(&session(&[csv("a", "X")]), &[("a.csv", CSV.as_bytes())]);
    let (status, _, body) = send(&app, request_with(body, false)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body_json(&body)["error"], "cross_origin");
}
