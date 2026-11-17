//! An axes-map calibration request: three moves along X, Y and Z, checked against the printer's
//! travel and turned into a plan and the G-code that runs each move.

use serde::Deserialize;
use wobblescope_core::axes_map::Mounting;
use wobblescope_session::meta::{AXES_MAP_DIRECTIONS, AxesMapRun, Mode};

use super::request::{
    Effective, Overrides, RequestError, Segment, TestKind, TestPlan, check_chips, refuse,
};
use crate::moonraker::settings::PrinterSettings;

/// Z height of the moves when the request gives none, in mm.
pub const DEFAULT_Z_HEIGHT: f64 = 20.0;
/// Speed of the measured moves when the request gives none, in mm/s.
pub const DEFAULT_SPEED: f64 = 80.0;
/// Acceleration of the measured moves when the request gives none, in mm/s².
pub const DEFAULT_ACCEL: f64 = 1500.0;
/// Speed of the positioning moves when the request gives none, in mm/s.
pub const DEFAULT_TRAVEL_SPEED: f64 = 120.0;

const MIN_SPEED: f64 = 20.0;
const MIN_ACCEL: f64 = 100.0;
const MIN_TRAVEL_SPEED: f64 = 20.0;

/// Distance the X and Y moves reach from the middle of the travel, in mm.
const HALF_SPAN: f64 = 15.0;
/// How far the Z move rises, in mm.
const Z_RISE: f64 = 30.0;

/// The time one move is expected to take, with its pauses and the tail, in seconds.
const SEGMENT_SECONDS: f64 = 6.0;

/// A chip in an axes-map request and where it is mounted.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChipChoice {
    /// The chip's config section.
    pub chip: String,
    /// Where the chip is attached.
    pub mounting: Mounting,
}

/// The request of an axes-map calibration. Unset settings take the defaults.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AxesMapRequest {
    /// The chips that record, with their mounting.
    pub chips: Vec<ChipChoice>,
    /// Z height of the moves, in mm.
    #[serde(default)]
    pub z_height: Option<f64>,
    /// Speed of the measured moves, in mm/s.
    #[serde(default)]
    pub speed: Option<f64>,
    /// Acceleration of the measured moves, in mm/s².
    #[serde(default)]
    pub accel: Option<f64>,
    /// Speed of the positioning moves, in mm/s.
    #[serde(default)]
    pub travel_speed: Option<f64>,
}

/// What the plan of an axes-map calibration adds to a [`TestPlan`].
#[derive(Debug, Clone, PartialEq)]
pub struct AxesMapPlan {
    /// The moves the run uses, as the session records them.
    pub run: AxesMapRun,
    /// Each chip and its mounting, in the order of the plan's chips.
    pub mountings: Vec<(String, Mounting)>,
    /// The start, the end of the X move, the end of the Y move and the end of the Z move.
    pub points: [[f64; 3]; 4],
}

fn setting(field: &str, value: Option<f64>, default: f64, min: f64) -> Result<f64, RequestError> {
    let value = value.unwrap_or(default);
    if !value.is_finite() {
        return refuse(field, "must be a number");
    }
    if value < min {
        return refuse(field, format!("must be at least {min}"));
    }
    Ok(value)
}

/// Checks `request` against what the printer reports and builds the plan.
///
/// # Errors
/// A [`RequestError`] naming the first field that is wrong.
pub fn plan_axes_map(
    request: &AxesMapRequest,
    printer: &PrinterSettings,
    max_segment_seconds: u32,
) -> Result<TestPlan, RequestError> {
    let chips: Vec<String> = request.chips.iter().map(|c| c.chip.clone()).collect();
    check_chips(&chips)?;
    let z_height = request.z_height.unwrap_or(DEFAULT_Z_HEIGHT);
    if !z_height.is_finite() {
        return refuse("z_height", "must be a number");
    }
    let speed = setting("speed", request.speed, DEFAULT_SPEED, MIN_SPEED)?;
    let accel = setting("accel", request.accel, DEFAULT_ACCEL, MIN_ACCEL)?;
    let travel_speed = setting(
        "travel_speed",
        request.travel_speed,
        DEFAULT_TRAVEL_SPEED,
        MIN_TRAVEL_SPEED,
    )?;
    let Some(limits) = printer.axis_limits else {
        return refuse(
            "axis_limits",
            "the toolhead's travel limits cannot be read from the printer",
        );
    };
    if f64::from(max_segment_seconds) < SEGMENT_SECONDS {
        return refuse(
            "max_segment_seconds",
            format!("a move takes about {SEGMENT_SECONDS} s, over the limit"),
        );
    }
    let mx = f64::midpoint(limits.x.min, limits.x.max);
    let my = f64::midpoint(limits.y.min, limits.y.max);
    let points = [
        [mx - HALF_SPAN, my - HALF_SPAN, z_height],
        [mx + HALF_SPAN, my - HALF_SPAN, z_height],
        [mx + HALF_SPAN, my + HALF_SPAN, z_height],
        [mx + HALF_SPAN, my + HALF_SPAN, z_height + Z_RISE],
    ];
    for point in &points {
        for (value, name, field, range) in [
            (point[0], "X", "axis_limits", limits.x),
            (point[1], "Y", "axis_limits", limits.y),
            (point[2], "Z", "z_height", limits.z),
        ] {
            if value < range.min || value > range.max {
                return refuse(
                    field,
                    format!(
                        "a move reaches {name} {value}, outside the toolhead's {name} travel of \
                         {} to {}",
                        range.min, range.max
                    ),
                );
            }
        }
    }
    let segments = AXES_MAP_DIRECTIONS
        .iter()
        .enumerate()
        .map(|(i, &axis)| Segment {
            axis,
            point: i,
            position: points[i],
            target: Some(points[i + 1]),
        })
        .collect();
    let run = AxesMapRun {
        speed,
        max_velocity: None,
        accel,
        z_height,
        travel_speed,
        max_z_velocity: printer.machine.max_z_velocity,
        max_z_accel: printer.machine.max_z_accel,
    };
    let mountings = request
        .chips
        .iter()
        .map(|c| (c.chip.clone(), c.mounting))
        .collect();
    Ok(TestPlan {
        kind: TestKind::AxesMap,
        chips,
        mode: Mode::Pulse,
        points: points[..3].to_vec(),
        effective: Effective {
            freq_start: 0.0,
            freq_end: 0.0,
            hz_per_sec: 0.0,
            accel_per_hz: 0.0,
            sweeping_accel: None,
            sweeping_period: None,
            move_speed: travel_speed,
        },
        overrides: Overrides::default(),
        segments,
        segment_seconds: SEGMENT_SECONDS,
        pulse_zeroes_period: false,
        axes_map: Some(AxesMapPlan {
            run,
            mountings,
            points,
        }),
        vibrations: None,
    })
}

impl TestPlan {
    /// The G-code that runs the measured move of `segment`, with a pause before and after so the
    /// recording holds a still lead-in and tail. Empty for a plan that is not an axes-map run or
    /// a segment without a target.
    #[must_use]
    pub fn segment_script(&self, segment: &Segment) -> String {
        let (Some(axes_map), Some(target)) = (&self.axes_map, segment.target) else {
            return String::new();
        };
        let [x, y, z] = std::array::from_fn::<_, 3, _>(|i| target[i] - segment.position[i]);
        format!(
            "SAVE_GCODE_STATE NAME=wobblescope\nG91\nG4 P500\nG1 X{x} Y{y} Z{z} F{}\nG4 P500\n\
             M400\nRESTORE_GCODE_STATE NAME=wobblescope",
            axes_map.run.speed * 60.0
        )
    }
}
