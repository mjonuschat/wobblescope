use axum::Json;
use axum::extract::Multipart;
use axum::extract::multipart::{Field, MultipartError};
use axum::http::StatusCode;
use serde::Deserialize;
use time::OffsetDateTime;

use super::error::ApiError;
use super::sessions::parse_axis;
use super::{IMPORT_LIMIT, ImportPermit, MAX_GROUP_FILES, Store, blocking};
use crate::csv_import::{GroupFile, import_group};
use crate::storage::index::Summary;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionPart {
    created_at: Option<String>,
    files: Vec<FileSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileSpec {
    chip: Option<String>,
    axis: Option<String>,
    point: Option<[f64; 3]>,
}

fn multipart_error(e: &MultipartError) -> ApiError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        too_large()
    } else {
        ApiError::invalid(e.body_text())
    }
}

/// The most bytes the `session` part may hold; 16 entries need far less.
const SESSION_PART_LIMIT: usize = 64 * 1024;

fn too_large() -> ApiError {
    ApiError::TooLarge {
        message: "the files add up to more than 64 MiB".to_owned(),
    }
}

async fn read_session_part(field: &mut Field<'_>) -> Result<Vec<u8>, ApiError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|e| multipart_error(&e))? {
        if bytes.len() + chunk.len() > SESSION_PART_LIMIT {
            return Err(ApiError::TooLarge {
                message: "the session part is larger than 64 KiB".to_owned(),
            });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn read_file(field: &mut Field<'_>, total: &mut usize) -> Result<Vec<u8>, ApiError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.chunk().await.map_err(|e| multipart_error(&e))? {
        *total += chunk.len();
        if *total > IMPORT_LIMIT {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) async fn import(
    Store(storage): Store,
    permit: ImportPermit,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Summary>), ApiError> {
    let mut first = multipart
        .next_field()
        .await
        .map_err(|e| multipart_error(&e))?
        .ok_or_else(|| ApiError::invalid("the request has no session part"))?;
    if first.name() != Some("session") {
        return Err(ApiError::invalid(
            "the first part must be named session and hold the JSON description",
        ));
    }
    let text = read_session_part(&mut first).await?;
    drop(first);
    let SessionPart { created_at, files } = serde_json::from_slice(&text)
        .map_err(|e| ApiError::invalid(format!("the session part is not valid: {e}")))?;
    if files.len() > MAX_GROUP_FILES {
        return Err(ApiError::invalid(format!(
            "at most {MAX_GROUP_FILES} files per import"
        )));
    }

    let listed = files.len();
    let mut group = Vec::with_capacity(listed);
    let mut total = 0;
    for (i, spec) in files.into_iter().enumerate() {
        let n = i + 1;
        let mut field = multipart
            .next_field()
            .await
            .map_err(|e| multipart_error(&e))?
            .ok_or_else(|| {
                ApiError::invalid(format!(
                    "the session part lists {listed} files but only {} file parts follow",
                    group.len()
                ))
            })?;
        if field.name() != Some("file") {
            return Err(ApiError::invalid(format!(
                "part {} must be named file",
                n + 1
            )));
        }
        let name = field
            .file_name()
            .map_or_else(|| format!("file {n}"), str::to_owned);
        let chip = spec
            .chip
            .filter(|c| !c.is_empty())
            .ok_or_else(|| ApiError::invalid(format!("file {n}: chip is required")))?;
        let axis = spec
            .axis
            .ok_or_else(|| ApiError::invalid(format!("file {n}: axis is required")))?;
        let axis = parse_axis(&axis)
            .map_err(|_| ApiError::invalid(format!("file {n}: axis must be x or y")))?;
        if spec.point.is_some_and(|p| p.iter().any(|v| !v.is_finite())) {
            return Err(ApiError::invalid(format!(
                "file {n}: point must be three finite numbers"
            )));
        }
        let bytes = read_file(&mut field, &mut total).await?;
        group.push(GroupFile {
            name,
            chip,
            axis,
            point: spec.point,
            bytes,
        });
    }
    if multipart
        .next_field()
        .await
        .map_err(|e| multipart_error(&e))?
        .is_some()
    {
        return Err(ApiError::invalid(
            "there are more file parts than the session part lists",
        ));
    }

    let summary = blocking(move || {
        let _permit = permit;
        Ok(import_group(
            &storage,
            group,
            created_at.as_deref(),
            OffsetDateTime::now_utc(),
        )?)
    })
    .await?;
    Ok((StatusCode::CREATED, Json(summary)))
}
