//! Raw accelerometer samples.

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
}
