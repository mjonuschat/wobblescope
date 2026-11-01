//! A test request: what the user asked for, checked against the printer's settings and turned
//! into a list of segments and the G-code that runs each one.

use serde::{Deserialize, Serialize};
use wobblescope_core::fit::Axis;
use wobblescope_session::meta::{Mode, SweepSettings};

use crate::moonraker::settings::PrinterSettings;

/// Seconds a segment needs besides the sweep itself: the move, the pause and the tail.
pub const SEGMENT_OVERHEAD_SECONDS: f64 = 15.0;

/// The longest frequency Klipper's resonance tester accepts, in Hz.
const MAX_FREQ: f64 = 300.0;

/// The speed of the move to a test point when the printer's own cannot be read, in mm/s.
pub const DEFAULT_MOVE_SPEED: f64 = 50.0;

/// The settings a test may override. Each is sent to Klipper only when set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Overrides {
    /// `FREQ_START`, in Hz.
    #[serde(default)]
    pub freq_start: Option<f64>,
    /// `FREQ_END`, in Hz.
    #[serde(default)]
    pub freq_end: Option<f64>,
    /// `HZ_PER_SEC`.
    #[serde(default)]
    pub hz_per_sec: Option<f64>,
    /// `ACCEL_PER_HZ`.
    #[serde(default)]
    pub accel_per_hz: Option<f64>,
    /// `SWEEPING_ACCEL`, in mm/s².
    #[serde(default)]
    pub sweeping_accel: Option<f64>,
    /// `SWEEPING_PERIOD`, in seconds.
    #[serde(default)]
    pub sweeping_period: Option<f64>,
    /// The speed of the move to a test point, in mm/s. Klipper is not told.
    #[serde(default)]
    pub move_speed: Option<f64>,
}

/// `POST /api/tests`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TestRequest {
    /// Config sections of the chips that record, for every axis.
    pub chips: Vec<String>,
    /// The axes to test, in order.
    pub axes: Vec<Axis>,
    /// Pulse or sweep.
    pub mode: Mode,
    /// The settings to override for this test.
    #[serde(default)]
    pub overrides: Overrides,
    /// The test points `[x, y, z]` in mm. Absent means the printer's default point.
    #[serde(default)]
    pub points: Option<Vec<[f64; 3]>>,
}

/// A request that cannot be run, and the field that is wrong.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{field}: {message}")]
pub struct RequestError {
    /// The request field.
    pub field: String,
    /// What is wrong.
    pub message: String,
}

fn refuse<T>(field: &str, message: impl Into<String>) -> Result<T, RequestError> {
    Err(RequestError {
        field: field.to_owned(),
        message: message.into(),
    })
}

/// The values a test runs with: the user's override or the printer's value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Effective {
    /// Lowest frequency, in Hz.
    pub freq_start: f64,
    /// Highest frequency, in Hz.
    pub freq_end: f64,
    /// Frequency change per second.
    pub hz_per_sec: f64,
    /// Acceleration per Hz.
    pub accel_per_hz: f64,
    /// The sweeping acceleration; known in sweep mode.
    pub sweeping_accel: Option<f64>,
    /// The sweeping period; known in sweep mode.
    pub sweeping_period: Option<f64>,
    /// The speed of the move to a point, in mm/s.
    pub move_speed: f64,
}

/// One (point, axis) run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// The axis shaken.
    pub axis: Axis,
    /// Index into the plan's points.
    pub point: usize,
    /// Where the toolhead stands, `[x, y, z]` in mm.
    pub position: [f64; 3],
}

/// A validated request: everything the runner needs to move and send commands.
#[derive(Debug, Clone, PartialEq)]
pub struct TestPlan {
    /// The chips that record.
    pub chips: Vec<String>,
    /// The test mode.
    pub mode: Mode,
    /// The test points.
    pub points: Vec<[f64; 3]>,
    /// The values the test runs with.
    pub effective: Effective,
    /// What the user overrode, which is all Klipper is told.
    pub overrides: Overrides,
    /// The segments in running order: points outermost, axes within.
    pub segments: Vec<Segment>,
    /// How long one segment is expected to take, in seconds.
    pub segment_seconds: f64,
    /// Whether the printer's own period must be switched off for a pulse test.
    pulse_zeroes_period: bool,
}

impl TestPlan {
    /// The G-code that runs `segment`, with its raw CSV named `name`.
    #[must_use]
    pub fn command(&self, segment: &Segment, name: &str) -> String {
        let axis = match segment.axis {
            Axis::X => "X",
            Axis::Y => "Y",
        };
        let [x, y, z] = segment.position;
        let chips = self.chips.join(",");
        let chips = if chips.contains(' ') {
            format!("\"{chips}\"")
        } else {
            chips
        };
        let mut command = format!(
            "TEST_RESONANCES AXIS={axis} POINT={x},{y},{z} OUTPUT=raw_data NAME={name} \
             CHIPS={chips}"
        );
        let o = &self.overrides;
        let send_sweeping = self.mode == Mode::Sweep;
        for (key, value) in [
            ("FREQ_START", o.freq_start),
            ("FREQ_END", o.freq_end),
            ("HZ_PER_SEC", o.hz_per_sec),
            ("ACCEL_PER_HZ", o.accel_per_hz),
            ("SWEEPING_ACCEL", o.sweeping_accel.filter(|_| send_sweeping)),
            (
                "SWEEPING_PERIOD",
                o.sweeping_period.filter(|_| send_sweeping),
            ),
        ] {
            if let Some(value) = value {
                command.push_str(&format!(" {key}={value}"));
            }
        }
        if self.pulse_zeroes_period {
            command.push_str(" SWEEPING_PERIOD=0");
        }
        command
    }

    /// The settings the session records. A pulse test stores no sweeping fields.
    #[must_use]
    pub fn sweep_settings(&self) -> SweepSettings {
        let e = &self.effective;
        SweepSettings {
            freq_start: e.freq_start,
            freq_end: e.freq_end,
            hz_per_sec: e.hz_per_sec,
            accel_per_hz: e.accel_per_hz,
            sweeping_accel: e.sweeping_accel.filter(|_| self.mode == Mode::Sweep),
            sweeping_period: e.sweeping_period.filter(|_| self.mode == Mode::Sweep),
        }
    }

    /// The time the whole test is expected to take, in seconds.
    #[must_use]
    pub fn total_seconds(&self) -> f64 {
        self.segment_seconds * self.segments.len() as f64
    }
}

/// Checks `request` against what the printer reports and builds the plan.
///
/// # Errors
/// A [`RequestError`] naming the first field that is wrong.
pub fn plan(
    request: &TestRequest,
    printer: &PrinterSettings,
    max_segment_seconds: u32,
) -> Result<TestPlan, RequestError> {
    check_chips(&request.chips)?;
    check_axes(&request.axes)?;
    let o = &request.overrides;
    check_overrides(o)?;
    let s = &printer.settings;
    let needed = |field: &str, over: Option<f64>, printed: Option<f64>| {
        over.or(printed).map_or_else(
            || {
                refuse(
                    field,
                    "cannot be read from the printer; set it for this test",
                )
            },
            Ok,
        )
    };
    let freq_start = needed("freq_start", o.freq_start, s.freq_start.value)?;
    let freq_end = needed("freq_end", o.freq_end, s.freq_end.value)?;
    let hz_per_sec = needed("hz_per_sec", o.hz_per_sec, s.hz_per_sec.value)?;
    let accel_per_hz = needed("accel_per_hz", o.accel_per_hz, s.accel_per_hz.value)?;
    if freq_end <= freq_start {
        return refuse(
            "freq_end",
            format!("must be above the start frequency {freq_start}"),
        );
    }
    let sweeping_accel = o.sweeping_accel.or(s.sweeping_accel.value);
    let sweeping_period = o.sweeping_period.or(s.sweeping_period.value);
    if request.mode == Mode::Sweep {
        if !sweeping_period.is_some_and(|p| p > 0.0) {
            return refuse(
                "sweeping_period",
                "sweep mode needs a sweeping period above 0; set it for this test",
            );
        }
        if sweeping_accel.is_none() {
            return refuse(
                "sweeping_accel",
                "sweep mode needs a sweeping acceleration; set it for this test",
            );
        }
    } else {
        if o.sweeping_accel.is_some() {
            return refuse("sweeping_accel", "applies only in sweep mode");
        }
        if o.sweeping_period.is_some_and(|p| p > 0.0) {
            return refuse("sweeping_period", "applies only in sweep mode");
        }
    }
    let points = match &request.points {
        Some(points) => points.clone(),
        None => match printer.default_point {
            Some(point) => vec![point],
            None => {
                return refuse(
                    "points",
                    "no test point was given and the printer's default cannot be worked out",
                );
            }
        },
    };
    if points.is_empty() {
        return refuse("points", "give at least one test point");
    }
    check_points(&points, printer)?;

    let segment_seconds = (freq_end - freq_start) / hz_per_sec + SEGMENT_OVERHEAD_SECONDS;
    if segment_seconds > f64::from(max_segment_seconds) {
        return refuse(
            "freq_end",
            format!(
                "one segment would take about {} s, over the limit of {max_segment_seconds} s \
                 (max_segment_seconds); lower the frequency range or raise hz_per_sec",
                segment_seconds.ceil()
            ),
        );
    }
    let segments = points
        .iter()
        .enumerate()
        .flat_map(|(point, position)| {
            request.axes.iter().map(move |axis| Segment {
                axis: *axis,
                point,
                position: *position,
            })
        })
        .collect();
    let pulse = request.mode == Mode::Pulse;
    Ok(TestPlan {
        chips: request.chips.clone(),
        mode: request.mode,
        points,
        effective: Effective {
            freq_start,
            freq_end,
            hz_per_sec,
            accel_per_hz,
            sweeping_accel,
            sweeping_period,
            move_speed: o
                .move_speed
                .or(s.move_speed.value)
                .unwrap_or(DEFAULT_MOVE_SPEED),
        },
        overrides: o.clone(),
        segments,
        segment_seconds,
        // Klipper's own period would make a pulse test sweep, so it is switched off unless the
        // effective period is known to be 0 already.
        pulse_zeroes_period: pulse && sweeping_period != Some(0.0),
    })
}

fn check_chips(chips: &[String]) -> Result<(), RequestError> {
    if chips.is_empty() {
        return refuse("chips", "choose at least one chip");
    }
    for (i, chip) in chips.iter().enumerate() {
        if chip.is_empty() || chip.chars().any(|c| c.is_control() || c == '"' || c == ',') {
            return refuse(
                "chips",
                format!("chip {chip:?} is not a config section name"),
            );
        }
        if chips[..i].contains(chip) {
            return refuse("chips", format!("chip {chip:?} is listed twice"));
        }
    }
    Ok(())
}

fn check_axes(axes: &[Axis]) -> Result<(), RequestError> {
    if axes.is_empty() {
        return refuse("axes", "choose at least one axis");
    }
    if (1..axes.len()).any(|i| axes[..i].contains(&axes[i])) {
        return refuse("axes", "an axis is listed twice");
    }
    Ok(())
}

fn check_overrides(o: &Overrides) -> Result<(), RequestError> {
    let finite = |field: &str, v: Option<f64>| match v {
        Some(v) if !v.is_finite() => refuse(field, "must be a number"),
        _ => Ok(()),
    };
    finite("freq_start", o.freq_start)?;
    finite("freq_end", o.freq_end)?;
    finite("hz_per_sec", o.hz_per_sec)?;
    finite("accel_per_hz", o.accel_per_hz)?;
    finite("sweeping_accel", o.sweeping_accel)?;
    finite("sweeping_period", o.sweeping_period)?;
    finite("move_speed", o.move_speed)?;
    if o.freq_start.is_some_and(|v| v < 1.0) {
        return refuse("freq_start", "must be at least 1 Hz");
    }
    if o.freq_end.is_some_and(|v| v > MAX_FREQ) {
        return refuse("freq_end", format!("must be at most {MAX_FREQ} Hz"));
    }
    if o.hz_per_sec.is_some_and(|v| v <= 0.0 || v > 2.0) {
        return refuse("hz_per_sec", "must be above 0 and at most 2");
    }
    if o.accel_per_hz.is_some_and(|v| v <= 0.0) {
        return refuse("accel_per_hz", "must be above 0");
    }
    if o.sweeping_accel.is_some_and(|v| v <= 0.0) {
        return refuse("sweeping_accel", "must be above 0");
    }
    if o.sweeping_period.is_some_and(|v| v < 0.0) {
        return refuse("sweeping_period", "must be at least 0");
    }
    if o.move_speed.is_some_and(|v| v <= 0.0) {
        return refuse("move_speed", "must be above 0");
    }
    Ok(())
}

fn check_points(points: &[[f64; 3]], printer: &PrinterSettings) -> Result<(), RequestError> {
    let Some(limits) = printer.axis_limits else {
        return refuse(
            "points",
            "the toolhead's travel limits cannot be read from the printer",
        );
    };
    for (i, point) in points.iter().enumerate() {
        for (value, name, range) in [
            (point[0], "X", limits.x),
            (point[1], "Y", limits.y),
            (point[2], "Z", limits.z),
        ] {
            if !value.is_finite() || value < range.min || value > range.max {
                return refuse(
                    "points",
                    format!(
                        "point {} has {name} {value}, outside the toolhead's {name} travel of \
                         {} to {}",
                        i + 1,
                        range.min,
                        range.max
                    ),
                );
            }
        }
    }
    Ok(())
}
