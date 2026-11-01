//! Turning a Klipper raw CSV into a stored session: resolve the axis, parse, then admit and
//! store. Nothing is admitted, and so nothing evicted, until the file has parsed.

use time::OffsetDateTime;
use wobblescope_core::fit::Axis;
use wobblescope_session::import::{
    ImportRequest, import_klipper_csv, suggest_hints_from_file_name,
};

use crate::capture::iso_utc;
use crate::capture::session_id::{base_id, first_free_id};
use crate::storage::estimate::estimate_from_samples;
use crate::storage::index::Summary;
use crate::storage::{Storage, StorageError};

/// The highest id suffix tried when ids from the same second collide.
/// What the caller says about the file being imported.
#[derive(Debug, Clone, Copy)]
pub struct ImportOptions<'a> {
    /// Klipper config section name of the chip that recorded the file.
    pub chip: &'a str,
    /// Test axis. When absent it is read from `file_name`.
    pub axis: Option<Axis>,
    /// Probe point in machine coordinates. When absent it is read from `file_name`.
    pub point: Option<[f64; 3]>,
    /// The uploaded file's name, kept in the metadata and used for the hints above.
    pub file_name: Option<&'a str>,
}

/// Why an import did not produce a session.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// No chip name was given.
    #[error("the chip name is required")]
    MissingChip,
    /// Neither the request nor the file name gave the axis.
    #[error("the axis is required: it was not given and the file name does not carry it")]
    MissingAxis,
    /// The body is not a valid Klipper raw CSV.
    #[error("invalid CSV: {0}")]
    InvalidCsv(String),
    /// Every id from `csv-<time>` to its `-99` suffix is taken.
    #[error("no free session id for this second")]
    NoFreeId,
    /// Admission refused the import, or storing it failed.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// Imports `body` as a new one-chip session named for `now`. The body is taken by value so
/// it can be freed as soon as it has been parsed.
///
/// The axis and point are resolved first, then the body is parsed, then the import is admitted
/// like a test and stored. A body that does not parse never causes an eviction. When the id for
/// this second is taken, `-2`, `-3` and so on up to `-99` are tried.
///
/// # Errors
/// [`ImportError`].
pub fn import_csv<B: AsRef<[u8]>>(
    storage: &Storage,
    body: B,
    options: &ImportOptions<'_>,
    now: OffsetDateTime,
) -> Result<Summary, ImportError> {
    if options.chip.is_empty() {
        return Err(ImportError::MissingChip);
    }
    let hints = options
        .file_name
        .map(suggest_hints_from_file_name)
        .unwrap_or_default();
    let axis = options
        .axis
        .or(hints.axis)
        .ok_or(ImportError::MissingAxis)?;

    let created_at = iso_utc(now);
    let base = base_id("csv", &created_at);

    let text = std::str::from_utf8(body.as_ref())
        .map_err(|e| ImportError::InvalidCsv(format!("the body is not UTF-8: {e}")))?;
    let imported = import_klipper_csv(
        text,
        &ImportRequest {
            id: base.clone(),
            created_at,
            chip_name: options.chip.to_owned(),
            axis,
            point: options.point.or(hints.point),
            original_file_name: options.file_name.map(str::to_owned),
        },
    )
    .map_err(|e| ImportError::InvalidCsv(e.to_string()))?;
    drop(body);

    // A parsed file holds at most 2^23 samples, so the estimate cannot overflow; saturating
    // would only make admission refuse it.
    let estimate = estimate_from_samples(imported.samples.len() as u64).unwrap_or(u64::MAX);
    let mut meta = imported.meta;
    let mut samples = imported.samples;
    let admitted = first_free_id(&base, |id| storage.admit(id, estimate))?;
    let Some((id, reservation)) = admitted else {
        return Err(ImportError::NoFreeId);
    };
    meta.id = id;
    Ok(storage.store(reservation, &meta, |_| Ok(std::mem::take(&mut samples)))?)
}
