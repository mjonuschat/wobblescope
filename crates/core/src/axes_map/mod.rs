//! Axes-map calibration: types and analysis.

pub(crate) mod map;
pub(crate) mod segment;

use serde::{Deserialize, Serialize};

use crate::samples::RawSamples;

/// Which part of the printer the accelerometer is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Mounting {
    /// Chip is on the toolhead (default).
    #[default]
    Toolhead,
    /// Chip is on the heated bed.
    Bed,
}

/// One of the three physical axes as reported by the chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ChipAxis {
    X,
    Y,
    Z,
}

/// Parameters for an axes-map measurement sweep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AxesMapParams {
    /// Toolhead speed during the sweep (mm/s).
    pub speed: f64,
    /// Toolhead acceleration during the sweep (mm/s²).
    pub accel: f64,
    /// Maximum Z velocity for bed-mounted chips (mm/s).
    pub max_z_velocity: Option<f64>,
    /// Maximum Z acceleration for bed-mounted chips (mm/s²).
    pub max_z_accel: Option<f64>,
}

/// Raw input for an axes-map analysis: three one-axis sweep recordings.
pub struct AxesMapInput {
    /// Chip name (matches Klipper config section).
    pub chip: String,
    /// One segment per printer axis (X sweep, Y sweep, Z sweep).
    pub segments: [RawSamples; 3],
    /// The axes_map value currently in the Klipper config, if any.
    pub configured_map: Option<String>,
    /// Whether the chip is on the toolhead or the bed.
    pub mounting: Mounting,
}

/// Whether a printer axis was directly observed or derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum AxisState {
    /// Directly measured.
    Measured,
    /// Computed from the other two.
    Virtual,
    /// Could not be determined.
    NotDetermined,
}

/// Detection result for one printer axis.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AxisMatch {
    /// How this axis was determined.
    pub state: AxisState,
    /// Which chip axis carries this printer axis (None when state is NotDetermined).
    pub chip_axis: Option<ChipAxis>,
    /// True when the chip axis is inverted relative to the printer axis.
    pub negative: bool,
    /// Detection confidence in [0, 1].
    pub confidence: f64,
    /// Estimated tilt of this axis in degrees.
    pub tilt_deg: f64,
    /// Peak signal power for each chip axis (X, Y, Z).
    pub peaks: [f64; 3],
    /// Velocity at the detected peak (mm/s).
    pub peak_velocity: f64,
    /// Time of the detected peak within the sweep (s).
    pub peak_time_s: f64,
}

/// Overall quality of the axes-map result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum MapState {
    /// All three axes were resolved.
    Complete,
    /// At least one axis was resolved.
    Partial,
    /// No motion was detected.
    NoMotion,
    /// The detected axes are ambiguous.
    Ambiguous,
}

/// Whether the detected map agrees with the configured map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ConfigCheck {
    /// No axes_map is set (or it is the identity).
    NoneConfigured,
    /// The configured map matches the detected map.
    AlreadyCorrect,
    /// The configured map does not match the detected map.
    DoesNotMatch,
}

/// Severity of a quality assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum QualityStatus {
    Ok,
    Warning,
    Error,
}

/// Whether the noise level is acceptable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum NoiseVerdict {
    Fine,
    High,
    TooHigh,
}

/// Human-readable quality assessment.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Quality {
    /// Overall quality severity.
    pub status: QualityStatus,
    /// Human-readable messages (warnings or errors).
    pub messages: Vec<String>,
}

/// Velocity-vs-time data for one sweep segment.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SegmentCurves {
    /// Time axis (s).
    pub t: Vec<f64>,
    /// Velocity signal for each chip axis (X, Y, Z).
    pub velocity: [Vec<f64>; 3],
}

/// Full result of an axes-map analysis.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AxesMapResult {
    /// Chip name from the input.
    pub chip: String,
    /// Overall map quality.
    pub state: MapState,
    /// Detected axes_map string (Klipper format), if the map is complete.
    pub map: Option<String>,
    /// The axes_map value from the Klipper config, if any.
    pub configured_map: Option<String>,
    /// Detection result per printer axis (X, Y, Z).
    pub axes: [AxisMatch; 3],
    /// Euler angles of the chip orientation (degrees), if determinable.
    pub euler_deg: Option<[f64; 3]>,
    /// Rotation matrix of the chip orientation, if determinable.
    pub rotation: Option<[[f64; 3]; 3]>,
    /// Estimated gravitational acceleration (m/s²).
    pub gravity: f64,
    /// Noise floor estimate.
    pub noise: f64,
    /// Noise level verdict.
    pub noise_verdict: NoiseVerdict,
    /// Quality assessment.
    pub quality: Quality,
    /// Whether the configured map matches the detected map.
    pub config_check: ConfigCheck,
    /// Velocity curves per segment for plotting.
    pub curves: [SegmentCurves; 3],
    /// Version of the analysis core that produced this result.
    pub core_version: String,
}

/// The axes-map golden file, decoded for unit tests of crate-private functions. The schema
/// duplicates the integration helper in `tests/common/mod.rs`, which unit tests cannot use.
#[cfg(test)]
pub(crate) mod golden {
    use std::collections::BTreeMap;
    use std::io::Read;

    use serde::Deserialize;

    use crate::samples::RawSamples;

    #[derive(Deserialize)]
    pub(crate) struct Golden {
        pub cases: BTreeMap<String, Case>,
    }

    #[derive(Deserialize)]
    pub(crate) struct Case {
        pub configured_map: Option<String>,
        pub segments: Vec<Segment>,
        pub expected: Expected,
    }

    #[derive(Deserialize)]
    pub(crate) struct Segment {
        pub t: Vec<f64>,
        pub x: Vec<f64>,
        pub y: Vec<f64>,
        pub z: Vec<f64>,
    }

    impl Segment {
        pub(crate) fn samples(&self) -> RawSamples {
            RawSamples {
                t: self.t.clone(),
                x: self.x.clone(),
                y: self.y.clone(),
                z: self.z.clone(),
            }
        }
    }

    #[derive(Deserialize)]
    pub(crate) struct Expected {
        pub per_segment: Vec<PerSegment>,
    }

    #[derive(Deserialize)]
    pub(crate) struct PerSegment {
        pub direction_vector: [f64; 3],
        pub actual_direction: [f64; 3],
        pub angle_error: f64,
        pub confidence: f64,
        pub noise_level: f64,
        pub peak_velocities: [f64; 3],
        pub gravity: f64,
    }

    /// Decode the embedded golden file.
    pub(crate) fn load() -> Golden {
        let bytes: &[u8] = include_bytes!("../../tests/golden/axes_map.json.zst");
        let mut decoder = structured_zstd::decoding::StreamingDecoder::new(bytes);
        let mut text = String::new();
        decoder.read_to_string(&mut text).unwrap();
        serde_json::from_str(&text).unwrap()
    }
}
