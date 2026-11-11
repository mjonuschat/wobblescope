//! Sample file download.

use std::io::Read;

use axum::body::{Body, Bytes};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use tokio::io::AsyncReadExt;

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
