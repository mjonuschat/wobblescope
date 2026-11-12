//! Pure DSP for resonance analysis. No I/O.

pub mod decimate;
pub mod eligibility;
pub mod error;
pub mod fit;
pub mod multichip;
pub mod numeric;
pub mod psd;
pub mod resonance;
pub mod samples;
pub mod score;
pub mod shaper;
pub mod signal;
pub mod spectrum;

pub use error::CoreError;
pub use psd::{CalibrationData, MAX_FREQ, MIN_FREQ, WINDOW_T_SEC};
pub use samples::RawSamples;

/// Version of this crate, recorded with every saved analysis result.
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
