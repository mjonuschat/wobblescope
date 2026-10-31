//! The admission size estimate: what a test is expected to occupy, before it runs.

/// Bytes per sample before compression: four `f64` columns.
const BYTES_PER_SAMPLE: u64 = 32;

/// Why an estimate cannot be computed.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum EstimateError {
    /// A rate or duration is NaN, infinite or negative. Without this check a NaN would cast to
    /// an estimate of 0 and admit anything.
    #[error("recording {index} has a rate or duration that is not a finite, non-negative number")]
    BadInput {
        /// Position of the recording in the input.
        index: usize,
    },
    /// The sample count does not fit in 64 bits.
    #[error("the estimate does not fit in 64 bits")]
    OutOfRange,
}

/// Estimated bytes for recordings given as (sample rate in Hz, duration in seconds): the
/// sample count rounded up per recording, times 32 bytes, times a safety factor of 1.5. Stored
/// files compress about 4.3 times, so the estimate is deliberately conservative.
///
/// # Errors
/// [`EstimateError`] for a bad input or a result beyond `u64`.
pub fn estimate_bytes(recordings: &[(f64, f64)]) -> Result<u64, EstimateError> {
    let mut samples: u64 = 0;
    for (index, &(rate, seconds)) in recordings.iter().enumerate() {
        let n = (rate * seconds).ceil();
        let ok = rate.is_finite() && seconds.is_finite() && rate >= 0.0 && seconds >= 0.0;
        if !ok {
            return Err(EstimateError::BadInput { index });
        }
        if n >= u64::MAX as f64 {
            return Err(EstimateError::OutOfRange);
        }
        samples = samples
            .checked_add(n as u64)
            .ok_or(EstimateError::OutOfRange)?;
    }
    // times 1.5, rounded up
    samples
        .checked_mul(BYTES_PER_SAMPLE * 3)
        .map(|b| b.div_ceil(2))
        .ok_or(EstimateError::OutOfRange)
}
