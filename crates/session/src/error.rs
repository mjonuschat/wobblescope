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
}
