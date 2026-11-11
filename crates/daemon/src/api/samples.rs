//! Sample file download.

use std::io::Read;

use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};

use super::error::{ApiError, ApiPath};
use super::{Store, blocking};

/// Serves one recording exactly as stored. The session stays leased while its file is read.
/// Nothing is cached: a deleted id can be written again with different data.
pub(super) async fn download(
    Store(storage): Store,
    ApiPath((id, chip, segment)): ApiPath<(String, u32, usize)>,
) -> Result<Response, ApiError> {
    let bytes = blocking(move || {
        let mut guard = storage.open_sample(&id, chip, segment)?;
        let mut bytes = Vec::new();
        guard
            .read_to_end(&mut bytes)
            .map_err(|e| ApiError::internal(format!("reading the sample file failed: {e}")))?;
        Ok(bytes)
    })
    .await?;
    Ok((
        [
            (CONTENT_TYPE, "application/octet-stream"),
            (CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}

/// `GET /api/sessions/{id}/download`: the session as a zip with its `meta.json` and sample files.
pub(super) async fn download_session(
    Store(storage): Store,
    ApiPath(id): ApiPath<String>,
) -> Result<Response, ApiError> {
    let name = id.clone();
    let bytes = blocking(move || Ok(storage.session_zip(&id)?)).await?;
    Ok((
        [
            (CONTENT_TYPE, "application/zip".to_owned()),
            (CACHE_CONTROL, "no-store".to_owned()),
            (
                CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}.zip\""),
            ),
        ],
        bytes,
    )
        .into_response())
}
