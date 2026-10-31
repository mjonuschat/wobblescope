//! Errors returned by the analysis core.

use thiserror::Error;

/// Everything the core can refuse to analyse. The core never panics on bad input.
#[derive(Debug, Error, PartialEq)]
pub enum CoreError {
    /// The time and acceleration columns have different lengths.
    #[error("sample columns have different lengths")]
    MismatchedColumns,
    /// The recording is shorter than one Welch window (or empty).
    #[error("too few samples: have {have}, need more than {need}")]
    TooFewSamples {
        /// Samples supplied.
        have: usize,
        /// Window length the recording must exceed.
        need: usize,
    },
    /// Timestamps are non-finite, not strictly increasing, or span no time.
    #[error("timestamps are not finite and strictly increasing")]
    InvalidTimestamps,
    /// An acceleration sample is NaN or infinite.
    #[error("non-finite acceleration sample")]
    NonFiniteSample,
    /// The signal has no power, so shaper quality is undefined.
    #[error("flat signal: no vibration power to analyse")]
    FlatSignal,
    /// The requested shaper name is not a built-in.
    #[error("unknown shaper {0:?}")]
    UnknownShaper(String),
    /// The shaper cannot be built with the given parameters.
    #[error("cannot initialise shaper: {0}")]
    ShaperInit(String),
    /// A parameter or dataset passed to the analysis is malformed (non-finite, out of range,
    /// inconsistent lengths).
    #[error("invalid input: {0}")]
    InvalidParameter(String),
}
