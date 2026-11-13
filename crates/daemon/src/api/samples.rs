//! Sample file download.

use std::io::Read;

use axum::body::{Body, Bytes};
use axum::http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_NONE_MATCH,
};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::io::AsyncReadExt;

use super::error::{ApiError, ApiPath};
use super::{Store, blocking};
use crate::results::container::sha256_hex;

/// Builds a sample file response with content-based caching. The browser is told to revalidate
/// every time (so a reused id never gets stale bytes), but if the content still matches its
/// stored etag, the server answers `304 Not Modified` without sending the body again.
pub(super) fn sample_response(headers: &HeaderMap, bytes: Vec<u8>) -> Response {
    let etag = format!("\"{}\"", sha256_hex(&bytes));
    if let Some(if_none_match) = headers.get(IF_NONE_MATCH)
        && let Ok(value) = if_none_match.to_str()
    {
        let tags: Vec<&str> = value.split(',').map(|s| s.trim()).collect();
        for tag in tags {
            let normalized = tag.strip_prefix("W/").unwrap_or(tag);
            if normalized == "*" || normalized == etag {
                return (
                    StatusCode::NOT_MODIFIED,
                    [
                        (ETAG, etag.clone()),
                        (CACHE_CONTROL, "private, no-cache".to_owned()),
                    ],
                )
                    .into_response();
            }
        }
    }
    (
        [
            (CONTENT_TYPE, "application/octet-stream".to_owned()),
            (CACHE_CONTROL, "private, no-cache".to_owned()),
            (ETAG, etag),
        ],
        bytes,
    )
        .into_response()
}

/// Serves one recording exactly as stored. The session stays leased while its file is read.
/// Cached by content, revalidated every time, so a reused id never gets stale bytes.
pub(super) async fn download(
    Store(storage): Store,
    ApiPath((id, chip, segment)): ApiPath<(String, u32, usize)>,
    headers: HeaderMap,
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
    Ok(sample_response(&headers, bytes))
}

/// `GET /api/sessions/{id}/download`: the session as a zip with its `meta.json` and sample files,
/// streamed from a temporary file so a large session does not sit in memory.
pub(super) async fn download_session(
    Store(storage): Store,
    ApiPath(id): ApiPath<String>,
) -> Result<Response, ApiError> {
    let name = id.clone();
    let (file, guard) = blocking(move || Ok(storage.session_zip(&id)?)).await?;
    let length = file
        .metadata()
        .map_err(|e| ApiError::internal(format!("reading the archive failed: {e}")))?
        .len();
    // The guard lives as long as the stream, so the next archive waits until this one is sent.
    let stream = futures_util::stream::unfold(
        (tokio::fs::File::from_std(file), guard),
        |(mut file, guard)| async {
            let mut chunk = vec![0u8; 64 * 1024];
            match file.read(&mut chunk).await {
                Ok(0) => None,
                Ok(n) => {
                    chunk.truncate(n);
                    Some((Ok::<_, std::io::Error>(Bytes::from(chunk)), (file, guard)))
                }
                Err(e) => Some((Err(e), (file, guard))),
            }
        },
    );
    Ok((
        [
            (CONTENT_TYPE, "application/zip".to_owned()),
            (CACHE_CONTROL, "no-store".to_owned()),
            (CONTENT_LENGTH, length.to_string()),
            (
                CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}.zip\""),
            ),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}
