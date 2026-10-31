//! Sample file download.

use std::io::Read;

use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
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
