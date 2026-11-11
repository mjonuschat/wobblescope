//! The HTTP API under `/api`: JSON in and out, every error the same shape, every storage call
//! on a blocking thread.

mod access;
mod error;
mod hold;
mod live;
mod origin;
mod printer;
mod results;
mod samples;
mod sessions;
mod settings;
mod status;
mod stream;
mod tests;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::{DefaultBodyLimit, FromRequestParts};
use axum::http::request::Parts;
use axum::middleware;
use axum::routing::{get, post};

pub use access::AccessGuard;
pub use error::ApiError;
pub use hold::{HoldClientMessage, HoldServerMessage, HoldSocketTimings};
pub use printer::{Homed, PrinterState};
pub use results::ResultList;
pub use sessions::{ImportQuery, ListQuery, PinRequest, SessionDetail, SessionList};
pub use status::{StatusResponse, StorageState, StorageStatus};
pub use tests::{StartedTest, StopDispatched};

use crate::capture::live::LiveHub;
use crate::capture::runner::{Runner, RunnerTimings};
use crate::capture::slot::TestSlot;
use crate::config::Settings;
use crate::moonraker::Moonraker;
use crate::startup::StorageCell;
use crate::storage::Storage;

/// The largest CSV an import accepts. A Klipper raw row is about 49 bytes, so this holds about
/// 430 seconds at 3.2 kHz.
const IMPORT_LIMIT: usize = 64 * 1024 * 1024;

const RESULT_LIMIT: usize = 16 * 1024 * 1024;

/// Lets one import run at a time. Cloning shares the gate.
#[derive(Clone, Default)]
pub struct ImportGate(Arc<AtomicBool>);

impl ImportGate {
    /// Takes the gate without waiting. `None` when an import holds it.
    pub fn try_acquire(&self) -> Option<ImportPermit> {
        (!self.0.swap(true, Ordering::AcqRel)).then(|| ImportPermit(self.0.clone()))
    }
}

/// Holds the [`ImportGate`] until dropped.
pub struct ImportPermit(Arc<AtomicBool>);

impl Drop for ImportPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// What every handler shares.
/// What the capture endpoints share with the runner. Clones share it.
#[derive(Clone)]
pub struct CaptureState {
    /// The one-test slot, the quarantine and the stop flags.
    pub slot: TestSlot,
    /// What the live socket shows.
    pub hub: LiveHub,
    /// The longest one sweep command may run, in seconds.
    pub max_segment_seconds: u32,
    /// The runner's timings.
    pub timings: RunnerTimings,
    /// The stream viewer's write and liveness limits.
    pub stream_timings: StreamTimings,
    /// The hold control socket's keep-alive and the hold's own limits.
    pub hold: HoldSocketTimings,
}

/// The limits that drop a stream viewer which is slow or gone.
#[derive(Debug, Clone, Copy)]
pub struct StreamTimings {
    /// How long one websocket write to a viewer may take before the viewer is dropped.
    pub write_timeout: Duration,
    /// How often the server pings a viewer.
    pub ping_interval: Duration,
    /// How long a viewer may send nothing at all before it is dropped.
    pub liveness_deadline: Duration,
}

impl Default for StreamTimings {
    fn default() -> Self {
        Self {
            write_timeout: stream::WRITE_TIMEOUT,
            ping_interval: stream::PING_INTERVAL,
            liveness_deadline: stream::LIVENESS_DEADLINE,
        }
    }
}

impl Default for CaptureState {
    fn default() -> Self {
        Self::new(crate::config::Settings::default().max_segment_seconds)
    }
}

impl CaptureState {
    /// Fresh capture state with the usual timings.
    #[must_use]
    pub fn new(max_segment_seconds: u32) -> Self {
        Self {
            slot: TestSlot::default(),
            hub: LiveHub::new(),
            max_segment_seconds,
            timings: RunnerTimings::default(),
            stream_timings: StreamTimings::default(),
            hold: HoldSocketTimings::default(),
        }
    }

    fn runner(&self, moonraker: Moonraker, storage: Storage) -> Runner {
        Runner {
            moonraker,
            storage,
            slot: self.slot.clone(),
            hub: self.hub.clone(),
            max_segment_seconds: self.max_segment_seconds,
            timings: self.timings,
        }
    }
}

#[derive(Clone)]
struct ApiState {
    storage: StorageCell,
    moonraker: Moonraker,
    imports: ImportGate,
    capture: CaptureState,
    settings: Arc<Settings>,
}

impl FromRequestParts<ApiState> for ImportPermit {
    type Rejection = ApiError;

    async fn from_request_parts(_: &mut Parts, state: &ApiState) -> Result<Self, ApiError> {
        state.imports.try_acquire().ok_or_else(|| ApiError::Busy {
            message: "another import is running".to_owned(),
        })
    }
}

/// The storage handle, or the 503 while storage is not open yet. Declared first in a
/// handler, it answers before any other extractor can reject the request.
struct Store(Storage);

impl FromRequestParts<ApiState> for Store {
    type Rejection = ApiError;

    async fn from_request_parts(_: &mut Parts, state: &ApiState) -> Result<Self, ApiError> {
        state
            .storage
            .get()
            .cloned()
            .map(Store)
            .ok_or_else(|| ApiError::StorageUnavailable {
                message: "the daemon is waiting for Moonraker to report where to store sessions"
                    .to_owned(),
            })
    }
}

/// Runs a storage call on a blocking thread.
async fn blocking<T, F>(f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ApiError> + Send + 'static,
{
    crate::storage::run_blocking(f)
        .await
        .map_err(ApiError::internal)?
}

async fn not_found() -> ApiError {
    ApiError::not_found("no such API path")
}

async fn method_not_allowed() -> ApiError {
    ApiError::MethodNotAllowed {
        message: "this path does not support that method".to_owned(),
    }
}

/// The API mounted at `/api`, with JSON answers for unknown paths and methods. Session
/// endpoints answer 503 `storage_unavailable` until `storage` holds a [`Storage`]. `moonraker`
/// feeds the status and chips endpoints.
pub fn mount(storage: StorageCell, moonraker: Moonraker) -> Router {
    mount_with(storage, moonraker, ImportGate::default())
}

/// [`mount`] with the gate that limits imports to one at a time, so a test can hold it.
pub fn mount_with(storage: StorageCell, moonraker: Moonraker, imports: ImportGate) -> Router {
    mount_capture(storage, moonraker, imports, CaptureState::default())
}

/// The API router with the capture state shared with the rest of the daemon and the default
/// settings.
pub fn mount_capture(
    storage: StorageCell,
    moonraker: Moonraker,
    imports: ImportGate,
    capture: CaptureState,
) -> Router {
    mount_configured(storage, moonraker, imports, capture, Settings::default())
}

/// [`mount_capture`] with the daemon's settings, which the live stream takes its defaults and
/// limits from.
pub fn mount_configured(
    storage: StorageCell,
    moonraker: Moonraker,
    imports: ImportGate,
    capture: CaptureState,
    settings: Settings,
) -> Router {
    let api = Router::new()
        .route(
            "/sessions",
            post(sessions::import)
                .layer(DefaultBodyLimit::max(IMPORT_LIMIT))
                .get(sessions::list),
        )
        .route(
            "/sessions/{id}",
            get(sessions::detail)
                .patch(sessions::pin)
                .delete(sessions::delete),
        )
        .route(
            "/sessions/{id}/samples/{chip}/{segment}",
            get(samples::download),
        )
        .route("/sessions/{id}/download", get(samples::download_session))
        .route(
            "/sessions/{id}/results",
            post(results::save).layer(DefaultBodyLimit::max(RESULT_LIMIT)),
        )
        .route("/results", get(results::list))
        .route(
            "/results/{rid}",
            get(results::detail).delete(results::delete),
        )
        .route(
            "/results/{rid}/samples/{chip}/{segment}",
            get(results::download),
        )
        .route("/storage", get(sessions::usage))
        .route("/status", get(status::status))
        .route("/chips", get(status::chips))
        .route("/chips/check", post(status::check_chips))
        .route("/settings", get(settings::settings))
        .route("/tests", post(tests::start))
        .route("/tests/{id}/emergency-stop", post(tests::emergency_stop))
        .route("/printer", get(printer::printer))
        .route("/printer/emergency-stop", post(printer::emergency_stop))
        .route("/printer/home", post(printer::home))
        .route("/live", get(live::live))
        .route("/stream", get(stream::stream))
        .route("/hold", get(hold::hold))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(middleware::from_fn(origin::guard))
        .with_state(ApiState {
            storage,
            moonraker,
            imports,
            capture,
            settings: Arc::new(settings),
        });
    Router::new().nest("/api", api)
}

/// Wraps a finished app, UI included, so that every request is checked by `guard` before any
/// route sees it. Serve the result with connect info.
pub fn guarded(app: Router, guard: AccessGuard) -> Router {
    app.layer(middleware::from_fn_with_state(guard, access::guard))
}
