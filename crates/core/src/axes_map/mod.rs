//! Axes-map calibration: types and analysis.

pub(crate) mod bed;
pub(crate) mod map;
pub(crate) mod orientation;
pub(crate) mod quality;
pub(crate) mod segment;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
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

/// The samples of `samples` inside `window`, or all of them without one.
fn within<'a>(
    samples: &'a RawSamples,
    window: Option<&std::ops::Range<f64>>,
) -> Result<std::borrow::Cow<'a, RawSamples>, CoreError> {
    Ok(match window {
        Some(range) => std::borrow::Cow::Owned(samples.slice(samples.window(range.clone())?)),
        None => std::borrow::Cow::Borrowed(samples),
    })
}

/// Raw input for an axes-map analysis: three one-axis sweep recordings.
pub struct AxesMapInput {
    /// Chip name (matches Klipper config section).
    pub chip: String,
    /// One segment per printer axis (X sweep, Y sweep, Z sweep).
    pub segments: [RawSamples; 3],
    /// The time range of each segment's measured move, in the clock of the sample timestamps.
    /// Only the samples inside it are analysed. A recording can start earlier than its window
    /// (the first one starts before the travel to the start point), and that earlier motion
    /// would count as part of the move. `None` analyses the whole recording.
    pub windows: [Option<std::ops::Range<f64>>; 3],
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
    /// Noise floor estimate (mm/s^2). Gravity above is in m/s^2.
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

/// Analyse three one-axis sweeps and work out how the chip is oriented.
///
/// This is a port of Shake&Tune's `AxesMapComputation.compute`, extended with the no-motion
/// guard (deviation N1) and bed-mounted chips (`bed::classify` picks the case).
///
/// # Errors
/// Any segment error from `process_segment`, or `InvalidParameter` when two or more axes
/// show no signal.
pub fn analyze_axes_map(
    input: &AxesMapInput,
    params: &AxesMapParams,
) -> Result<AxesMapResult, CoreError> {
    let inverse = map::inverse_matrix(input.configured_map.as_deref());
    let [s0, s1, s2] = &input.segments;
    let [w0, w1, w2] = &input.windows;
    let (s0, s1, s2) = (
        within(s0, w0.as_ref())?,
        within(s1, w1.as_ref())?,
        within(s2, w2.as_ref())?,
    );
    let segments = [
        segment::process_segment(&s0, inverse.as_ref())?,
        segment::process_segment(&s1, inverse.as_ref())?,
        segment::process_segment(&s2, inverse.as_ref())?,
    ];

    let case = bed::classify(input.mounting, &segments, params)?;
    let gravity = segments.iter().map(|s| s.gravity).sum::<f64>() / 3.0;
    let noise = segments.iter().map(|s| s.noise_level).sum::<f64>() / 3.0;

    let outcome = match case {
        bed::MotionCase::NoMotion => no_motion_outcome(params),
        bed::MotionCase::Partial { moving } => partial_outcome(&segments, &moving, input.mounting),
        bed::MotionCase::Full => {
            complete_outcome(&segments, None, input.mounting, noise, gravity, params)
        }
        bed::MotionCase::Complete { missing } => complete_outcome(
            &segments,
            Some(missing),
            input.mounting,
            noise,
            gravity,
            params,
        ),
    };
    let Outcome {
        state,
        map,
        axes,
        euler_deg,
        rotation,
        quality,
    } = outcome;

    let config_check = match &map {
        Some(detected) => map::config_check(input.configured_map.as_deref(), detected),
        None => ConfigCheck::NoneConfigured,
    };

    let [c0, c1, c2] = segments.map(|s| s.curves);
    Ok(AxesMapResult {
        chip: input.chip.clone(),
        state,
        map,
        configured_map: input.configured_map.clone(),
        axes,
        euler_deg,
        rotation,
        gravity: gravity / 1000.0,
        noise,
        noise_verdict: quality::noise_verdict(noise),
        quality,
        config_check,
        curves: [c0, c1, c2],
        core_version: crate::CORE_VERSION.to_owned(),
    })
}

/// The parts of an `AxesMapResult` that depend on which axes could be determined.
struct Outcome {
    state: MapState,
    map: Option<String>,
    axes: [AxisMatch; 3],
    euler_deg: Option<[f64; 3]>,
    rotation: Option<[[f64; 3]; 3]>,
    quality: Quality,
}

fn undetermined_axis() -> AxisMatch {
    AxisMatch {
        state: AxisState::NotDetermined,
        chip_axis: None,
        negative: false,
        confidence: 0.0,
        tilt_deg: 0.0,
        peaks: [0.0; 3],
        peak_velocity: 0.0,
        peak_time_s: 0.0,
    }
}

fn no_motion_outcome(params: &AxesMapParams) -> Outcome {
    let mut messages = vec!["No motion seen by this chip.".to_owned()];
    if params.max_z_velocity.is_none() || params.max_z_accel.is_none() {
        messages.push(
            "Z cannot be judged without Klipper's Z limits (max_z_velocity and max_z_accel)."
                .to_owned(),
        );
    }
    Outcome {
        state: MapState::NoMotion,
        map: None,
        axes: std::array::from_fn(|_| undetermined_axis()),
        euler_deg: None,
        rotation: None,
        quality: Quality {
            status: QualityStatus::Error,
            messages,
        },
    }
}

/// Sign applied to every measured direction: a bed chip moves opposite to the toolhead.
fn mounting_sign(mounting: Mounting) -> f64 {
    match mounting {
        Mounting::Toolhead => 1.0,
        Mounting::Bed => -1.0,
    }
}

fn partial_outcome(
    segments: &[segment::SegmentResult; 3],
    moving: &[usize],
    mounting: Mounting,
) -> Outcome {
    let sign = mounting_sign(mounting);
    let axes = std::array::from_fn(|i| {
        if !moving.contains(&i) {
            return undetermined_axis();
        }
        let s = &segments[i];
        let k = quality::dominant_index(&s.direction_vector);
        AxisMatch {
            state: AxisState::Measured,
            chip_axis: Some(chip_axis_from_index(k)),
            negative: s.direction_vector[k] * sign <= 0.0,
            confidence: s.confidence,
            tilt_deg: s.angle_error,
            peaks: s.peak_velocities,
            peak_velocity: s.peak_velocities[k],
            peak_time_s: s.peak_time_s,
        }
    });
    let names: Vec<&str> = moving.iter().map(|&i| ["X", "Y", "Z"][i]).collect();
    Outcome {
        state: MapState::Partial,
        map: None,
        axes,
        euler_deg: None,
        rotation: None,
        quality: Quality {
            status: QualityStatus::Warning,
            messages: vec![format!(
                "Only the {} axis was seen moving; the other axes could not be determined.",
                names.join(" and ")
            )],
        },
    }
}

/// All three axes are measured (`virtual_axis` is `None`) or two are measured and one is
/// completed by cross product.
fn complete_outcome(
    segments: &[segment::SegmentResult; 3],
    virtual_axis: Option<usize>,
    mounting: Mounting,
    noise: f64,
    gravity: f64,
    params: &AxesMapParams,
) -> Outcome {
    let sign = mounting_sign(mounting);
    let mut directions = segments
        .each_ref()
        .map(|s| s.direction_vector.map(|c| c * sign));
    let mut actual = segments
        .each_ref()
        .map(|s| s.actual_direction.map(|c| c * sign));
    let mut confidences = segments.each_ref().map(|s| s.confidence);
    let mut angle_errors = segments.each_ref().map(|s| s.angle_error);

    let mut degenerate = false;
    if let Some(missing) = virtual_axis {
        if orientation::complete_axis(&mut directions, missing) {
            actual[missing] = directions[missing];
        } else {
            degenerate = true;
        }
        confidences[missing] = 0.0;
        angle_errors[missing] = 0.0;
    }

    let quality = quality::validate(
        &directions,
        confidences,
        angle_errors,
        noise,
        gravity,
        params.accel,
    );

    let chip_axes: [usize; 3] = directions.map(|d| quality::dominant_index(&d));
    let map = map::format_map(&std::array::from_fn(|i| {
        let k = chip_axes[i];
        (chip_axis_from_index(k), directions[i][k] <= 0.0)
    }));
    let state = if degenerate || map.is_none() {
        MapState::Ambiguous
    } else {
        MapState::Complete
    };
    let map = if state == MapState::Ambiguous {
        None
    } else {
        map
    };

    let (rotation, euler_deg) = if state == MapState::Ambiguous {
        (None, None)
    } else {
        let rotation = orientation::nearest_rotation(actual);
        (Some(rotation), Some(orientation::euler_degrees(&rotation)))
    };

    let axes: [AxisMatch; 3] = std::array::from_fn(|i| {
        let k = chip_axes[i];
        let is_virtual = virtual_axis == Some(i);
        AxisMatch {
            state: if is_virtual {
                AxisState::Virtual
            } else {
                AxisState::Measured
            },
            chip_axis: Some(chip_axis_from_index(k)),
            negative: directions[i][k] <= 0.0,
            confidence: confidences[i],
            tilt_deg: angle_errors[i],
            peaks: if is_virtual {
                [0.0; 3]
            } else {
                segments[i].peak_velocities
            },
            peak_velocity: if is_virtual {
                0.0
            } else {
                segments[i].peak_velocities[k]
            },
            peak_time_s: if is_virtual {
                0.0
            } else {
                segments[i].peak_time_s
            },
        }
    });

    Outcome {
        state,
        map,
        axes,
        euler_deg,
        rotation,
        quality,
    }
}

fn chip_axis_from_index(index: usize) -> ChipAxis {
    match index {
        0 => ChipAxis::X,
        1 => ChipAxis::Y,
        _ => ChipAxis::Z,
    }
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
