//! The HTTP API under `/api`: JSON in and out, every error the same shape, every storage call
//! on a blocking thread.

mod error;
mod samples;
mod sessions;

use axum::Router;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::routing::get;

pub use error::ApiError;
pub use sessions::{ListQuery, SessionDetail, SessionList};

use crate::storage::Storage;

/// What every handler shares.
#[derive(Clone)]
struct ApiState {
    storage: Option<Storage>,
}

/// The storage handle, or the 503 for a daemon with no storage directory. Declared first in a
/// handler, it answers before any other extractor can reject the request.
struct Store(Storage);

impl FromRequestParts<ApiState> for Store {
    type Rejection = ApiError;

    async fn from_request_parts(_: &mut Parts, state: &ApiState) -> Result<Self, ApiError> {
        state
            .storage
            .clone()
            .map(Store)
            .ok_or_else(|| ApiError::StorageDisabled {
                message: "no storage directory is configured".to_owned(),
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

/// The API mounted at `/api`, with JSON answers for unknown paths and methods. `storage` is
/// `None` when the daemon runs without a storage directory.
pub fn mount(storage: Option<Storage>) -> Router {
    let api = Router::new()
        .route("/sessions", get(sessions::list))
        .route("/sessions/{id}", get(sessions::detail))
        .route(
            "/sessions/{id}/samples/{chip}/{segment}",
            get(samples::download),
        )
        .route("/storage", get(sessions::usage))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(ApiState { storage });
    Router::new().nest("/api", api)
}
