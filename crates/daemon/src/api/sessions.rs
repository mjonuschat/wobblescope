//! Listing, detail and storage use.

use axum::Json;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use wobblescope_core::fit::Axis;
use wobblescope_session::meta::SessionMeta;

use super::error::{ApiBody, ApiError, ApiJson, ApiPath, ApiQuery};
use super::{ApiState, ImportPermit, Store, blocking};
use crate::csv_import::{ImportOptions, import_csv};
use crate::storage::Usage;
use crate::storage::index::{ListFilter, Summary, created_at_unix};

/// The query of `GET /api/sessions`. Every value arrives as text so a bad one can be refused
/// by name.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ListQuery {
    /// Sessions created at or after this RFC 3339 time.
    pub created_from: Option<String>,
    /// Sessions created before this RFC 3339 time.
    pub created_to: Option<String>,
    /// Sessions with a chip of this name.
    pub chip: Option<String>,
    /// Sessions recorded on a printer of this name.
    pub printer: Option<String>,
    /// `true` for pinned sessions, `false` for unpinned ones.
    pub pinned: Option<String>,
}

impl ListQuery {
    fn filter(self) -> Result<ListFilter, ApiError> {
        let time = |name: &str, v: Option<String>| {
            v.map(|text| {
                created_at_unix(&text)
                    .map_err(|_| ApiError::invalid(format!("{name} is not an RFC 3339 time")))
            })
            .transpose()
        };
        let pinned = self
            .pinned
            .map(|v| match v.as_str() {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(ApiError::invalid("pinned must be true or false")),
            })
            .transpose()?;
        Ok(ListFilter {
            created_from: time("created_from", self.created_from)?,
            created_to: time("created_to", self.created_to)?,
            chip: self.chip,
            printer: self.printer,
            pinned,
        })
    }
}

/// The body of `GET /api/sessions`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SessionList {
    /// Matching sessions, newest first.
    pub sessions: Vec<Summary>,
}

/// The body of `GET /api/sessions/{id}`.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SessionDetail {
    /// The listing summary.
    pub summary: Summary,
    /// `meta.json` as stored.
    pub meta: SessionMeta,
}

pub(super) async fn list(
    Store(storage): Store,
    ApiQuery(query): ApiQuery<ListQuery>,
) -> Result<Json<SessionList>, ApiError> {
    let filter = query.filter()?;
    let sessions = blocking(move || Ok(storage.list(&filter))).await?;
    Ok(Json(SessionList { sessions }))
}

pub(super) async fn detail(
    Store(storage): Store,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<SessionDetail>, ApiError> {
    let (summary, meta) = blocking(move || Ok(storage.session(&id)?)).await?;
    Ok(Json(SessionDetail { summary, meta }))
}

pub(super) async fn usage(Store(storage): Store) -> Result<Json<Usage>, ApiError> {
    Ok(Json(blocking(move || Ok(storage.usage()?)).await?))
}

/// The body of `PATCH /api/sessions/{id}`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PinRequest {
    /// Whether the session should be pinned.
    pub pinned: bool,
}

pub(super) async fn pin(
    Store(storage): Store,
    ApiPath(id): ApiPath<String>,
    ApiJson(request): ApiJson<PinRequest>,
) -> Result<Json<Summary>, ApiError> {
    let summary = blocking(move || {
        if request.pinned {
            storage.pin(&id)?;
        } else {
            storage.unpin(&id)?;
        }
        storage
            .get(&id)
            .ok_or_else(|| ApiError::not_found(format!("no session {id}")))
    })
    .await?;
    Ok(Json(summary))
}

pub(super) async fn delete(
    Store(storage): Store,
    ApiPath(id): ApiPath<String>,
) -> Result<StatusCode, ApiError> {
    blocking(move || Ok(storage.delete(&id)?)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The query of `POST /api/sessions`. Every value arrives as text so a bad one can be refused
/// by name.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ImportQuery {
    /// Klipper config section name of the chip that recorded the file. Required.
    pub chip: Option<String>,
    /// `x` or `y`. Read from `file_name` when absent.
    pub axis: Option<String>,
    /// The probe point as `x,y,z` in mm. Read from `file_name` when absent.
    pub point: Option<String>,
    /// The uploaded file's name.
    pub file_name: Option<String>,
}

fn parse_axis(text: &str) -> Result<Axis, ApiError> {
    match text.to_ascii_lowercase().as_str() {
        "x" => Ok(Axis::X),
        "y" => Ok(Axis::Y),
        _ => Err(ApiError::invalid("axis must be x or y")),
    }
}

fn parse_point(text: &str) -> Result<[f64; 3], ApiError> {
    let bad = || ApiError::invalid("point must be three finite numbers as x,y,z");
    let parts: Vec<f64> = text
        .split(',')
        .map(|p| p.trim().parse::<f64>().map_err(|_| bad()))
        .collect::<Result<_, _>>()?;
    match parts[..] {
        [x, y, z] if parts.iter().all(|v| v.is_finite()) => Ok([x, y, z]),
        _ => Err(bad()),
    }
}

/// The import options after validation. As an extractor it runs before the import permit is
/// taken and before the body is read, so a bad request never holds up a valid one.
pub(super) struct ImportParams {
    chip: String,
    axis: Option<Axis>,
    point: Option<[f64; 3]>,
    file_name: Option<String>,
}

impl FromRequestParts<ApiState> for ImportParams {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &ApiState) -> Result<Self, ApiError> {
        let ApiQuery(query) = ApiQuery::<ImportQuery>::from_request_parts(parts, state).await?;
        let chip = query
            .chip
            .filter(|c| !c.is_empty())
            .ok_or_else(|| ApiError::invalid("chip is required"))?;
        Ok(Self {
            chip,
            axis: query.axis.as_deref().map(parse_axis).transpose()?,
            point: query.point.as_deref().map(parse_point).transpose()?,
            file_name: query.file_name,
        })
    }
}

pub(super) async fn import(
    Store(storage): Store,
    params: ImportParams,
    permit: ImportPermit,
    ApiBody(body): ApiBody,
) -> Result<(StatusCode, Json<Summary>), ApiError> {
    let summary = blocking(move || {
        let _permit = permit;
        let options = ImportOptions {
            chip: &params.chip,
            axis: params.axis,
            point: params.point,
            file_name: params.file_name.as_deref(),
        };
        Ok(import_csv(
            &storage,
            body,
            &options,
            OffsetDateTime::now_utc(),
        )?)
    })
    .await?;
    Ok((StatusCode::CREATED, Json(summary)))
}
