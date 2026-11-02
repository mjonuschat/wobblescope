//! Shared helpers: real sessions written through the session crate. The session crate's own
//! test helpers are not exported, so this keeps a minimal one-chip-per-name meta builder.
#![allow(dead_code)]

use std::path::Path;

pub mod fake_bridge;
pub mod fake_moonraker;
pub mod fake_printer;
pub mod sim_printer;

use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::fit::Axis;
use wobblescope_session::SessionError;
use wobblescope_session::fs::write_session;
use wobblescope_session::meta::{
    ChipMeta, Mode, Outcome, PrinterInfo, ProbePoint, SegmentFile, SegmentMeta, SessionMeta,
    Source, TimeDomain, sample_file_name,
};

/// What a test session looks like. Defaults: a complete capture with one chip.
#[derive(Clone)]
pub struct Spec {
    pub id: String,
    pub created_at: String,
    pub source: Source,
    pub outcome: Outcome,
    pub mode: Option<Mode>,
    pub chips: Vec<String>,
    pub printer: Option<String>,
    pub samples: usize,
}

impl Spec {
    pub fn new(id: &str, created_at: &str) -> Self {
        Self {
            id: id.to_owned(),
            created_at: created_at.to_owned(),
            source: Source::Capture,
            outcome: Outcome::Complete,
            mode: None,
            chips: vec!["adxl345".to_owned()],
            printer: None,
            samples: 200,
        }
    }

    pub fn chips(mut self, chips: &[&str]) -> Self {
        self.chips = chips.iter().map(|c| (*c).to_owned()).collect();
        self
    }

    pub fn printer(mut self, name: &str) -> Self {
        self.printer = Some(name.to_owned());
        self
    }

    pub fn samples(mut self, n: usize) -> Self {
        self.samples = n;
        self
    }

    pub fn meta(&self) -> SessionMeta {
        let chips = self
            .chips
            .iter()
            .map(|name| ChipMeta {
                name: name.clone(),
                chip_type: None,
                nominal_rate_hz: None,
                observed_rate_hz: None,
                axes_map: None,
                mounting: None,
                health: Health::Ok,
                health_notes: vec![],
            })
            .collect();
        let files = (0..self.chips.len())
            .map(|chip| SegmentFile {
                chip: chip as u32,
                sample_file: sample_file_name(chip, 0),
                sample_count: self.samples as u32,
                coverage: Coverage::Complete,
                health: None,
                health_notes: Vec::new(),
            })
            .collect();
        SessionMeta {
            format_version: 1,
            id: self.id.clone(),
            created_at: self.created_at.clone(),
            source: self.source.clone(),
            time_domain: TimeDomain::KlipperPrintTime,
            mode: self.mode,
            settings: None,
            printer: self.printer.as_ref().map(|n| PrinterInfo {
                name: Some(n.clone()),
                klipper_version: None,
                mcu_firmware: vec![],
            }),
            outcome: self.outcome.clone(),
            notes: vec![],
            probe_points: vec![ProbePoint { position: None }],
            chips,
            segments: vec![SegmentMeta {
                axis: Axis::X,
                point: 0,
                window: None,
                files,
            }],
        }
    }
}

pub fn samples(n: usize, seed: f64) -> RawSamples {
    let mut s = RawSamples::default();
    for i in 0..n {
        s.t.push(41230.322878 + i as f64 * 0.000313);
        s.x.push((i as f64 * 0.37 + seed).sin() * 300.0);
        s.y.push(seed);
        s.z.push(-9806.022374);
    }
    s
}

pub fn source(f: &SegmentFile) -> Result<RawSamples, SessionError> {
    Ok(samples(f.sample_count as usize, f64::from(f.chip)))
}

/// Seals the session `spec` describes under `root`.
pub fn seal(root: &Path, spec: &Spec) {
    write_session(root, &spec.meta(), source).unwrap();
}

use std::path::PathBuf;
use std::sync::Arc;

use wobblescope::config::Settings;
use wobblescope::startup::StorageCell;
use wobblescope::storage::{OpenReport, Probe, Storage};

pub fn sessions_dir(data: &Path) -> PathBuf {
    data.join("sessions")
}

/// Seals `spec` under `<data>/sessions`, creating the directory when needed.
pub fn seal_in(data: &Path, spec: &Spec) {
    std::fs::create_dir_all(sessions_dir(data)).unwrap();
    seal(&sessions_dir(data), spec);
}

/// A probe that reports `free` bytes whatever the path.
pub fn probe(free: u64) -> Probe {
    Arc::new(move |_| Ok(free))
}

pub fn settings(cap: u64, headroom: u64) -> Settings {
    Settings {
        retention_cap_bytes: cap,
        free_space_headroom_bytes: headroom,
        ..Settings::default()
    }
}

/// Opens storage on `data` with plenty of free space.
pub fn open(data: &Path, cap: u64) -> (Storage, OpenReport) {
    Storage::open(data, &settings(cap, 0), probe(u64::MAX / 2)).unwrap()
}

/// Sizes of the sessions `ids`, read back from storage.
pub fn size_of(storage: &Storage, id: &str) -> u64 {
    storage.get(id).unwrap().size_bytes
}

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// A storage cell that holds `storage`, or is still empty.
pub fn cell(storage: Option<Storage>) -> StorageCell {
    let cell = wobblescope::startup::new_cell();
    if let Some(storage) = storage {
        let _ = cell.set(storage);
    }
    cell
}

/// A Moonraker handle nothing publishes to: connecting, no chips.
pub fn idle_moonraker() -> wobblescope::moonraker::Moonraker {
    wobblescope::moonraker::channel().1
}

/// The API router on `storage`, with an idle Moonraker.
pub fn app(storage: Option<Storage>) -> Router {
    wobblescope::api::mount(cell(storage), idle_moonraker())
}

/// Sends one request through `app` and returns the status, headers and body.
pub async fn send(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
    let response = app.clone().oneshot(request).await.unwrap();
    let (parts, body) = response.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes().to_vec();
    (parts.status, parts.headers, bytes)
}

/// A request with no body.
pub fn request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

/// Sends a bodiless request and parses the answer as JSON.
pub async fn json(app: &Router, method: &str, uri: &str) -> (StatusCode, serde_json::Value) {
    let (status, _, body) = send(app, request(method, uri)).await;
    let value = serde_json::from_slice(&body)
        .unwrap_or_else(|e| panic!("{method} {uri} gave {status} and a non-JSON body: {e}"));
    (status, value)
}

/// Sends a bodiless POST and parses the answer as JSON.
pub async fn json_post(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    json(app, "POST", uri).await
}

/// A plan for a pulse test of `chips` on the given axes at the printer's default point, built
/// from the real settings fixture.
pub fn test_plan(
    chips: &[&str],
    axes: &[wobblescope_core::fit::Axis],
) -> wobblescope::capture::request::TestPlan {
    use wobblescope::capture::request::{Overrides, TestRequest, plan};
    use wobblescope::moonraker::settings::parse_settings;
    let printer = parse_settings(
        &fake_printer::fixture("real_settings.json"),
        &fake_printer::fixture("real_printer_info.json"),
    )
    .unwrap();
    let request = TestRequest {
        chips: chips.iter().map(|c| (*c).to_owned()).collect(),
        axes: axes.to_vec(),
        mode: wobblescope_session::meta::Mode::Pulse,
        overrides: Overrides::default(),
        points: None,
    };
    plan(&request, &printer, 600).unwrap()
}
