//! The one JSON error body every `/api` failure uses, and how storage errors map onto it.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Path, Query, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::capture::CheckFailure;
use crate::csv_import::ImportError;
use crate::storage::StorageError;

/// An error response: `{"error": "<code>", "message": "<text>"}`, plus byte counts for the two
/// refusals that have them. The code is the stable part; the message is for people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "error", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        need: u64,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        cap: u64,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        pinned_bytes: u64,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        evictable_bytes: u64,
    },
    /// The disk cannot hold the import plus headroom.
    InsufficientDisk {
        message: String,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        need: u64,
        #[cfg_attr(feature = "ts", ts(type = "number"))]
        available: u64,
    },
    /// One or more pre-flight checks failed; every failed check is listed.
    PreflightFailed {
        message: String,
        failures: Vec<CheckFailure>,
    },
    /// No running test has this id, and no finished test with uncertain motion.
    TestNotRunning { message: String },
    /// A write from another origin.
    CrossOrigin { message: String },
    /// The client's address is not in Moonraker's `trusted_clients`.
    UntrustedClient { message: String },
    /// The `Host` of the request is not on the allowlist.
    HostNotAllowed { message: String },
    /// Moonraker requires logins, which the daemon cannot check yet.
    LoginsRequired { message: String },
    /// The daemon has not read Moonraker's `trusted_clients`, so it admits only loopback.
    AccessUnavailable { message: String },
    /// Storage is not open yet: the daemon is waiting for Moonraker to report the data directory.
    StorageUnavailable { message: String },
    /// Moonraker cannot answer: not connected, or it answered with an error.
    MoonrakerUnavailable { message: String },
    /// Anything else that went wrong on the daemon.
    Internal { message: String },
}

impl ApiError {
    /// A refusal that lists every failed check.
    pub fn preflight(failures: Vec<CheckFailure>) -> Self {
        let message = match failures.as_slice() {
            [one] => one.message.clone(),
            many => format!("{} checks failed", many.len()),
        };
        Self::PreflightFailed { message, failures }
    }

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
            Self::NotFound { .. } | Self::TestNotRunning { .. } => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed { .. } => StatusCode::METHOD_NOT_ALLOWED,
            Self::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            Self::InvalidCsv { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::TooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Busy { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Pinned { .. } | Self::InUse { .. } | Self::PreflightFailed { .. } => {
                StatusCode::CONFLICT
            }
            Self::CapExceeded { .. } | Self::InsufficientDisk { .. } => {
                StatusCode::INSUFFICIENT_STORAGE
            }
            Self::CrossOrigin { .. }
            | Self::UntrustedClient { .. }
            | Self::HostNotAllowed { .. }
            | Self::LoginsRequired { .. } => StatusCode::FORBIDDEN,
            Self::StorageUnavailable { .. }
            | Self::MoonrakerUnavailable { .. }
            | Self::AccessUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
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
            StorageError::NotFound(_)
            | StorageError::NoRecording { .. }
            | StorageError::ResultNotFound(_) => Self::NotFound { message },
            StorageError::BadRequest(_) => Self::InvalidRequest { message },
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
            | StorageError::BadSession(_)
            | StorageError::SessionChanged(_)
            | StorageError::ResultUnusable { .. } => Self::Internal { message },
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

impl From<ImportError> for ApiError {
    fn from(e: ImportError) -> Self {
        let message = e.to_string();
        match e {
            ImportError::MissingChip | ImportError::MissingAxis => Self::InvalidRequest { message },
            ImportError::InvalidCsv(_) => Self::InvalidCsv { message },
            ImportError::NoFreeId => Self::Internal { message },
            ImportError::Storage(e) => e.into(),
        }
    }
}

/// `axum::Json` that rejects with the JSON error body. A body that does not parse, has the
/// wrong shape or lacks a JSON content type is a 400.
pub struct ApiJson<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        Json::<T>::from_request(req, state)
            .await
            .map(|j| Self(j.0))
            .map_err(|r| ApiError::invalid(r.body_text()))
    }
}

/// The request body as bytes, up to the route's body limit. Over the limit is a 413.
pub struct ApiBody(pub Bytes);

impl<S: Send + Sync> FromRequest<S> for ApiBody {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        Bytes::from_request(req, state)
            .await
            .map(Self)
            .map_err(|r| {
                if r.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    ApiError::TooLarge {
                        message: "the request body is over the size limit".to_owned(),
                    }
                } else {
                    ApiError::invalid(r.body_text())
                }
            })
    }
}
