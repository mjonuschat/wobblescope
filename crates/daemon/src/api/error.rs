//! The one JSON error body every `/api` failure uses, and how storage errors map onto it.

use axum::Json;
use axum::extract::{FromRequestParts, Path, Query};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::storage::StorageError;

/// An error response: `{"error": "<code>", "message": "<text>"}`, plus byte counts for the two
/// refusals that have them. The code is the stable part; the message is for people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ApiError {
    /// No such session, recording or path.
    NotFound { message: String },
    /// The path exists but not for this method.
    MethodNotAllowed { message: String },
    /// A bad path, query or body.
    InvalidRequest { message: String },
    /// The uploaded file is not a valid Klipper raw CSV.
    InvalidCsv { message: String },
    /// The upload is over the size limit.
    TooLarge { message: String },
    /// Another import is running.
    Busy { message: String },
    /// The session is pinned.
    Pinned { message: String },
    /// The session is being downloaded.
    InUse { message: String },
    /// The retention cap cannot fit the import even after evicting everything eligible.
    CapExceeded {
        message: String,
        need: u64,
        cap: u64,
        pinned_bytes: u64,
        evictable_bytes: u64,
    },
    /// The disk cannot hold the import plus headroom.
    InsufficientDisk {
        message: String,
        need: u64,
        available: u64,
    },
    /// A write from another origin.
    CrossOrigin { message: String },
    /// No storage directory is configured.
    StorageDisabled { message: String },
    /// Anything else that went wrong on the daemon.
    Internal { message: String },
}

impl ApiError {
    /// A 400 with `message`.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidRequest {
            message: message.into(),
        }
    }

    /// A 404 with `message`.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    /// A 500 with `message`.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    /// The HTTP status of this error.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed { .. } => StatusCode::METHOD_NOT_ALLOWED,
            Self::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            Self::InvalidCsv { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Busy { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Pinned { .. } | Self::InUse { .. } => StatusCode::CONFLICT,
            Self::CapExceeded { .. } | Self::InsufficientDisk { .. } => {
                StatusCode::INSUFFICIENT_STORAGE
            }
            Self::CrossOrigin { .. } => StatusCode::FORBIDDEN,
            Self::StorageDisabled { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status(), Json(self)).into_response()
    }
}

impl From<StorageError> for ApiError {
    fn from(e: StorageError) -> Self {
        let message = e.to_string();
        match e {
            StorageError::NotFound(_) | StorageError::NoRecording { .. } => {
                Self::NotFound { message }
            }
            StorageError::Pinned(_) => Self::Pinned { message },
            StorageError::InUse(_) => Self::InUse { message },
            StorageError::CapExceeded {
                need,
                cap,
                pinned_bytes,
                evictable_bytes,
            } => Self::CapExceeded {
                message,
                need,
                cap,
                pinned_bytes,
                evictable_bytes,
            },
            StorageError::InsufficientDisk { need, available } => Self::InsufficientDisk {
                message,
                need,
                available,
            },
            StorageError::Session(_)
            | StorageError::Pins(_)
            | StorageError::Io(_)
            | StorageError::IdTaken(_)
            | StorageError::BadSession(_) => Self::Internal { message },
        }
    }
}

/// `axum::extract::Path` that rejects with the JSON error body.
pub struct ApiPath<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for ApiPath<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|p| Self(p.0))
            .map_err(|r| ApiError::invalid(r.body_text()))
    }
}

/// `axum::extract::Query` that rejects with the JSON error body.
pub struct ApiQuery<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequestParts<S> for ApiQuery<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|q| Self(q.0))
            .map_err(|r| ApiError::invalid(r.body_text()))
    }
}
