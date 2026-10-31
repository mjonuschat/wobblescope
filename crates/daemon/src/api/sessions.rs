//! Listing, detail and storage use.

use axum::Json;
use serde::{Deserialize, Serialize};
use wobblescope_session::meta::SessionMeta;

use super::error::{ApiError, ApiPath, ApiQuery};
use super::{Store, blocking};
use crate::storage::Usage;
use crate::storage::index::{ListFilter, Summary, created_at_unix};

/// The query of `GET /api/sessions`. Every value arrives as text so a bad one can be refused
/// by name.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
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
pub struct SessionList {
    /// Matching sessions, newest first.
    pub sessions: Vec<Summary>,
}

/// The body of `GET /api/sessions/{id}`.
#[derive(Debug, Serialize)]
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
