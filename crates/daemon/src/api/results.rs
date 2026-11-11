use axum::Json;
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use time::OffsetDateTime;

use super::error::{ApiError, ApiJson, ApiPath};
use super::{Store, blocking};
use crate::results::record::{ResultDetail, ResultSummary, SaveRequest};

/// The body of `GET /api/results`, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultList {
    /// Every saved result, usable or not.
    pub results: Vec<ResultSummary>,
}

pub(super) async fn save(
    Store(storage): Store,
    ApiPath(id): ApiPath<String>,
    ApiJson(request): ApiJson<SaveRequest>,
) -> Result<(StatusCode, Json<ResultSummary>), ApiError> {
    let summary =
        blocking(move || Ok(storage.save_result(&id, request, OffsetDateTime::now_utc())?)).await?;
    Ok((StatusCode::CREATED, Json(summary)))
}

pub(super) async fn list(Store(storage): Store) -> Result<Json<ResultList>, ApiError> {
    let results = blocking(move || Ok(storage.list_results()?)).await?;
    Ok(Json(ResultList { results }))
}

pub(super) async fn detail(
    Store(storage): Store,
    ApiPath(rid): ApiPath<String>,
) -> Result<Json<ResultDetail>, ApiError> {
    Ok(Json(blocking(move || Ok(storage.result(&rid)?)).await?))
}

/// `GET /api/results/{rid}/samples/{chip}/{segment}`: the sample file as stored. Not cached,
/// since a deleted result id can be reused.
pub(super) async fn download(
    Store(storage): Store,
    ApiPath((rid, chip, segment)): ApiPath<(String, u32, usize)>,
) -> Result<Response, ApiError> {
    let bytes = blocking(move || Ok(storage.result_sample(&rid, chip, segment)?)).await?;
    Ok((
        [
            (CONTENT_TYPE, "application/octet-stream"),
            (CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}

pub(super) async fn delete(
    Store(storage): Store,
    ApiPath(rid): ApiPath<String>,
) -> Result<StatusCode, ApiError> {
    blocking(move || Ok(storage.delete_result(&rid)?)).await?;
    Ok(StatusCode::NO_CONTENT)
}
