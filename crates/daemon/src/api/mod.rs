//! The HTTP API under `/api`: JSON in and out, every error the same shape, every storage call
//! on a blocking thread.

mod error;
mod origin;
mod samples;
mod sessions;
mod settings;
mod status;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::extract::{DefaultBodyLimit, FromRequestParts};
use axum::http::request::Parts;
use axum::middleware;
use axum::routing::{get, post};

pub use error::ApiError;
pub use sessions::{ImportQuery, ListQuery, PinRequest, SessionDetail, SessionList};
pub use status::{StatusResponse, StorageState, StorageStatus};

use crate::moonraker::Moonraker;
use crate::startup::StorageCell;
use crate::storage::Storage;

/// The largest CSV an import accepts. A Klipper raw row is about 49 bytes, so this holds about
/// 430 seconds at 3.2 kHz.
const IMPORT_LIMIT: usize = 64 * 1024 * 1024;

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
#[derive(Clone)]
struct ApiState {
    storage: StorageCell,
    moonraker: Moonraker,
    imports: ImportGate,
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
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::internal(format!("storage task failed: {e}")))?
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
        .route("/storage", get(sessions::usage))
        .route("/status", get(status::status))
        .route("/chips", get(status::chips))
        .route("/chips/check", post(status::check_chips))
        .route("/settings", get(settings::settings))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(middleware::from_fn(origin::guard))
        .with_state(ApiState {
            storage,
            moonraker,
            imports,
        });
    Router::new().nest("/api", api)
}
