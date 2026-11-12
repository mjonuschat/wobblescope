//! Errors returned by the session crate.

use thiserror::Error;

/// Everything the session crate can refuse to read or write. Hostile or damaged input is an
/// error, never a panic and never a shorter recording.
#[derive(Debug, Error)]
pub enum SessionError {
    /// The sample columns have different lengths.
    #[error("sample columns have different lengths")]
    MismatchedColumns,
    /// The payload does not start with the sample file magic.
    #[error("not a sample file: wrong magic")]
    BadMagic,
    /// The sample file was written by a newer format version.
    #[error("unsupported sample file version {found}")]
    UnsupportedSampleVersion {
        /// Version found in the file.
        found: u32,
    },
    /// The sample count exceeds [`crate::codec::MAX_SAMPLES`].
    #[error("sample count {claimed} exceeds the limit")]
    TooManySamples {
        /// Count claimed by the header (or requested for encoding).
        claimed: u64,
    },
    /// The compressed frame declares a decoder window larger than the limit.
    #[error("compressed frame declares a window of {requested} bytes, over the limit {limit}")]
    WindowTooLarge {
        /// Window the frame declares.
        requested: u64,
        /// Limit in force.
        limit: u64,
    },
    /// The file ends before the header or the claimed samples are complete.
    #[error("sample file is truncated")]
    Truncated,
    /// Bytes follow the last claimed sample or the compressed frame.
    #[error("unexpected data after the last sample")]
    TrailingData,
    /// The compressed frame carries no content checksum, so damage could go unnoticed.
    #[error("sample file frame has no content checksum")]
    MissingChecksum,
    /// The compressed frame is damaged or fails its checksum.
    #[error("corrupt sample file: {0}")]
    Corrupt(String),
    /// `meta.json` was written by a newer format version.
    #[error("unsupported session format version {found}")]
    UnsupportedFormatVersion {
        /// Version found in the metadata.
        found: u32,
    },
    /// `meta.json` is not valid JSON for the metadata shape.
    #[error("invalid metadata JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A metadata field breaks a format rule.
    #[error("invalid metadata field {field}: {reason}")]
    Invalid {
        /// Path of the offending field, such as `segments[0].window`.
        field: String,
        /// Which rule it breaks.
        reason: String,
    },
    /// A loaded recording does not hold the number of samples `meta.json` records for it.
    #[error("{file} holds {found} samples, metadata says {expected}")]
    SampleCountMismatch {
        /// The sample file name.
        file: String,
        /// Count recorded in the metadata.
        expected: u32,
        /// Count actually loaded.
        found: usize,
    },
    /// A session with this id is already sealed.
    #[error("session {id} already exists")]
    AlreadyExists {
        /// The session id.
        id: String,
    },
    /// No sealed session has this id: the directory or its `meta.json` is missing.
    #[error("session {id} is not sealed")]
    NotSealed {
        /// The session id.
        id: String,
    },
    /// A path inside a session is not a regular file or directory, or names a file the metadata
    /// does not list.
    #[error("refusing {0}: not a regular file inside the session")]
    UnsafePath(String),
    /// Reading or writing the session directory failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// A row of a Klipper raw CSV cannot be imported.
    #[error("CSV line {line}: {reason}")]
    CsvRow {
        /// 1-based line number in the text.
        line: usize,
        /// What is wrong with the row.
        reason: String,
    },
    /// The CSV holds fewer than the two samples an import needs.
    #[error("CSV holds {found} samples, an import needs at least 2")]
    CsvTooShort {
        /// Samples found.
        found: usize,
    },
    /// A group import was given no files.
    #[error("no files to import")]
    ImportNoFiles,
    /// Two files of a group name the same chip, axis and point.
    #[error(
        "chip {chip} appears twice for axis {axis:?} at {point:?}; give the files different points"
    )]
    ImportDuplicate {
        /// The chip named twice.
        chip: String,
        /// The axis of the segment.
        axis: wobblescope_core::fit::Axis,
        /// The probe point, `None` when unknown.
        point: Option<[f64; 3]>,
    },
    /// Two chips of one segment have time ranges that do not overlap.
    #[error("these files were not recorded in the same run")]
    ImportNotSameRun {
        /// One chip of the pair.
        chip_a: String,
        /// The other chip of the pair.
        chip_b: String,
        /// The axis of the segment.
        axis: wobblescope_core::fit::Axis,
    },
}
