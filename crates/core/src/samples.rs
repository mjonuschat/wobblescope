//! Raw accelerometer samples.

use crate::error::CoreError;
use crate::numeric::strictly_increasing;

/// Timestamped three-axis samples. All columns have the same length.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawSamples {
    /// Timestamps in seconds.
    pub t: Vec<f64>,
    /// X acceleration.
    pub x: Vec<f64>,
    /// Y acceleration.
    pub y: Vec<f64>,
    /// Z acceleration.
    pub z: Vec<f64>,
}

impl RawSamples {
    /// Number of samples (length of the time column).
    pub fn len(&self) -> usize {
        self.t.len()
    }

    /// True when there are no samples.
    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }

    /// The samples in `range` as a new recording.
    ///
    /// # Panics
    /// If `range` is out of bounds, like slice indexing.
    pub fn slice(&self, range: std::ops::Range<usize>) -> Self {
        Self {
            t: self.t[range.clone()].to_vec(),
            x: self.x[range.clone()].to_vec(),
            y: self.y[range.clone()].to_vec(),
            z: self.z[range].to_vec(),
        }
    }

    /// Index range of the samples whose timestamps lie in `range` (start inclusive, end
    /// exclusive). The range is empty when the window lies outside the recording.
    ///
    /// # Errors
    /// [`CoreError::InvalidTimestamps`] when the window is not a finite, non-empty interval or
    /// the timestamps are not finite and strictly increasing.
    pub fn window(&self, range: std::ops::Range<f64>) -> Result<std::ops::Range<usize>, CoreError> {
        if !(range.start.is_finite() && range.end.is_finite() && range.start < range.end) {
            return Err(CoreError::InvalidTimestamps);
        }
        if !strictly_increasing(&self.t) {
            return Err(CoreError::InvalidTimestamps);
        }
        let start = self.t.partition_point(|&t| t < range.start);
        let end = self.t.partition_point(|&t| t < range.end);
        Ok(start..end)
    }
}
