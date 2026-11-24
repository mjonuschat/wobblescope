//! A vibrations-profile request: back-and-forth moves at rising speeds along two angles, checked
//! against the printer and turned into a plan and the G-code that runs each step.

use serde::Deserialize;
use wobblescope_core::direction::Direction;
use wobblescope_session::codec::MAX_SAMPLES;
use wobblescope_session::meta::{ANGULAR_STEPS, Mode, VibrationsRun, expected_speeds};

use super::request::{
    Effective, Overrides, RequestError, Segment, TestKind, TestPlan, check_chips, refuse,
};
use crate::moonraker::settings::PrinterSettings;

/// Side of the square the toolhead moves over when the request gives none, in mm.
pub const DEFAULT_SIZE: f64 = 100.0;
/// Z height of the moves when the request gives none, in mm.
pub const DEFAULT_Z_HEIGHT: f64 = 20.0;
/// Highest speed when the request gives none, in mm/s.
pub const DEFAULT_MAX_SPEED: f64 = 200.0;
/// Speed increase between steps when the request gives none, in mm/s.
pub const DEFAULT_SPEED_INCREMENT: f64 = 2.0;
/// Acceleration of the moves when the request gives none, in mm/s².
pub const DEFAULT_ACCEL: f64 = 3000.0;
/// Speed of the positioning moves when the request gives none, in mm/s.
pub const DEFAULT_TRAVEL_SPEED: f64 = 120.0;
/// Start speed when the request gives none, and the lowest start speed allowed, in mm/s.
pub const MIN_SPEED: f64 = 2.0;

const MIN_SIZE: f64 = 50.0;
const MIN_MAX_SPEED: f64 = 10.0;
const MIN_SPEED_INCREMENT: f64 = 1.0;
const MIN_ACCEL: f64 = 100.0;
const MIN_TRAVEL_SPEED: f64 = 20.0;

/// Klipper's highest sample rate, taken for a chip whose rate is unknown, in Hz.
const UNKNOWN_RATE_HZ: f64 = 3200.0;
const SAMPLE_MARGIN: f64 = 1.1;
/// Dwell after the moves of a step, in seconds.
const STEP_DWELL_SECONDS: f64 = 0.3;

/// The request of a vibrations profile. Unset settings take Shake&Tune's defaults.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VibrationsRequest {
    /// The chips that record. Every chip records every step.
    pub chips: Vec<String>,
    /// Side of the square the toolhead moves over, in mm.
    #[serde(default)]
    pub size: Option<f64>,
    /// Z height of the moves, in mm.
    #[serde(default)]
    pub z_height: Option<f64>,
    /// First speed, in mm/s. At least 2 and below `max_speed`; 2 when unset.
    #[serde(default)]
    pub min_speed: Option<f64>,
    /// Highest speed, in mm/s.
    #[serde(default)]
    pub max_speed: Option<f64>,
    /// Speed increase between steps, in mm/s.
    #[serde(default)]
    pub speed_increment: Option<f64>,
    /// Acceleration of the moves, in mm/s².
    #[serde(default)]
    pub accel: Option<f64>,
    /// Speed of the positioning moves, in mm/s.
    #[serde(default)]
    pub travel_speed: Option<f64>,
    /// Degrees between the measured directions: 45, 22.5 or 15. The motor pair alone when unset.
    #[serde(default)]
    pub angular_step: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VibrationsPlan {
    pub run: VibrationsRun,
    /// The angles in degrees, in running order: the motor pair first.
    pub angles: Vec<f64>,
    /// The speeds of every angle in mm/s, lowest first.
    pub speeds: Vec<f64>,
    /// The middle of the XY travel, `[x, y]` in mm.
    pub mid: [f64; 2],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepGeometry {
    /// The start point `[x, y]` in mm.
    pub start: [f64; 2],
    /// The far end `[x, y]` in mm.
    pub end: [f64; 2],
    pub repeats: u32,
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

/// The factor on the segment length at `speed`. Shake&Tune shortens the segments below 100 mm/s
/// from the full size at 100 mm/s down to a fifth of it.
fn length_multiplier(speed: f64) -> f64 {
    if speed < 100.0 {
        1.0 / 5.0 + 4.0 / 5.0 * speed / 100.0
    } else {
        1.0
    }
}

fn repeats(speed: f64) -> u32 {
    if speed < 150.0 {
        1
    } else if speed < 250.0 {
        2
    } else {
        3
    }
}

/// Checks `request` against what the printer reports and builds the plan. `chip_rates` holds the
/// nominal sample rate of each chip in the request, `None` when unknown.
///
/// A vibrations profile has two segments of many steps each, so `max_segment_seconds` does not
/// apply to it and is not read.
///
/// # Errors
/// A [`RequestError`] naming the first field that is wrong.
pub fn plan_vibrations(
    request: &VibrationsRequest,
    printer: &PrinterSettings,
    _max_segment_seconds: u32,
    chip_rates: &[Option<f64>],
) -> Result<TestPlan, RequestError> {
    check_chips(&request.chips)?;
    if let Some(step) = request.angular_step
        && !ANGULAR_STEPS.contains(&step)
    {
        return refuse("angular_step", "must be 45, 22.5 or 15");
    }
    // Shake&Tune refuses to run without an input shaper object, and the profile is meant to be
    // measured with the printer's shaper on.
    if !printer.input_shaper_configured {
        return refuse(
            "input_shaper",
            "the input shaper is not configured; run the shaper calibration first",
        );
    }
    let size = setting("size", request.size, DEFAULT_SIZE, MIN_SIZE)?;
    let z_height = request.z_height.unwrap_or(DEFAULT_Z_HEIGHT);
    if !z_height.is_finite() {
        return refuse("z_height", "must be a number");
    }
    // Shake&Tune always starts at 2 mm/s. A request can set the start speed, which is
    // refused below 2 mm/s and at or above the end speed.
    let min_speed = setting("min_speed", request.min_speed, MIN_SPEED, MIN_SPEED)?;
    let max_speed = setting(
        "max_speed",
        request.max_speed,
        DEFAULT_MAX_SPEED,
        MIN_MAX_SPEED,
    )?;
    if min_speed >= max_speed {
        return refuse(
            "min_speed",
            format!("must be below the end speed of {max_speed} mm/s"),
        );
    }
    let speed_increment = setting(
        "speed_increment",
        request.speed_increment,
        DEFAULT_SPEED_INCREMENT,
        MIN_SPEED_INCREMENT,
    )?;
    let accel = setting("accel", request.accel, DEFAULT_ACCEL, MIN_ACCEL)?;
    let travel_speed = setting(
        "travel_speed",
        request.travel_speed,
        DEFAULT_TRAVEL_SPEED,
        MIN_TRAVEL_SPEED,
    )?;
    // Shake&Tune checks size / (max_speed / 60) < 0.25, which is 60 times too lax. This
    // checks size / max_speed >= 0.25 and refuses otherwise.
    if size / max_speed < 0.25 {
        return refuse(
            "max_speed",
            "the size of the movement is too small for this speed; increase size or decrease \
             max_speed",
        );
    }
    // The analysis needs at least two speeds. The start is below the end, and the end
    // speed is always listed, so every accepted range has two.
    let Some(kinematics) = printer.machine.kinematics.clone() else {
        return refuse(
            "kinematics",
            "the printer's kinematics cannot be read from the printer",
        );
    };
    let run = VibrationsRun {
        size,
        z_height,
        max_speed,
        speed_increment,
        accel,
        travel_speed,
        min_speed,
        kinematics,
        max_velocity: printer.machine.max_velocity,
        angular_step: request.angular_step,
    };
    let Some(angles) = run.planned_angles() else {
        return refuse(
            "kinematics",
            format!(
                "{} is not supported; a vibrations profile needs Cartesian, CoreXY or CoreXZ \
                 kinematics",
                run.kinematics
            ),
        );
    };
    let Some(limits) = printer.axis_limits else {
        return refuse(
            "axis_limits",
            "the toolhead's travel limits cannot be read from the printer",
        );
    };
    let mid = [
        f64::midpoint(limits.x.min, limits.x.max),
        f64::midpoint(limits.y.min, limits.y.max),
    ];
    let half = size / 2.0;
    // The four corners of the largest step, at full size whatever the direction.
    let corners = [
        [mid[0] - half, mid[1] - half],
        [mid[0] + half, mid[1] - half],
        [mid[0] + half, mid[1] + half],
        [mid[0] - half, mid[1] + half],
    ];
    for corner in &corners {
        for (value, name, field, range) in [
            (corner[0], "X", "size", limits.x),
            (corner[1], "Y", "size", limits.y),
            (z_height, "Z", "z_height", limits.z),
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
    let speeds = expected_speeds(&run);
    let plan = VibrationsPlan {
        run,
        angles,
        speeds,
        mid,
    };
    let segment_seconds = estimated_angle_seconds(&plan);
    let rate = chip_rates
        .iter()
        .map(|rate| rate.unwrap_or(UNKNOWN_RATE_HZ))
        .fold(0.0, f64::max);
    if segment_seconds * rate * SAMPLE_MARGIN > MAX_SAMPLES as f64 {
        return refuse(
            "max_speed",
            format!(
                "one angle would take about {} s and record more than the {MAX_SAMPLES} samples a \
                 sample file holds at {rate} Hz; lower max_speed or raise speed_increment",
                segment_seconds.ceil()
            ),
        );
    }
    let segments = plan
        .angles
        .iter()
        .map(|&angle| {
            let start = step_geometry(&plan, angle, plan.speeds[0]).start;
            Segment {
                axis: Direction::from_degrees(angle),
                point: 0,
                position: [start[0], start[1], z_height],
                target: None,
            }
        })
        .collect();
    Ok(TestPlan {
        kind: TestKind::Vibrations,
        chips: request.chips.clone(),
        mode: Mode::Pulse,
        points: vec![[mid[0], mid[1], z_height]],
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
        segment_seconds,
        pulse_zeroes_period: false,
        axes_map: None,
        vibrations: Some(plan),
    })
}

#[must_use]
pub fn step_geometry(plan: &VibrationsPlan, angle: f64, speed: f64) -> StepGeometry {
    let multiplier = length_multiplier(speed);
    let radians = angle.to_radians();
    let d = [
        plan.run.size / 2.0 * radians.cos() * multiplier,
        plan.run.size / 2.0 * radians.sin() * multiplier,
    ];
    StepGeometry {
        start: [plan.mid[0] - d[0], plan.mid[1] - d[1]],
        end: [plan.mid[0] + d[0], plan.mid[1] + d[1]],
        repeats: repeats(speed),
    }
}

/// A feed rate in mm/min for a speed in mm/s, rounded to three decimals with no trailing zeros.
pub(super) fn feed(speed: f64) -> String {
    let text = format!("{:.3}", speed * 60.0);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[must_use]
pub fn step_script(plan: &VibrationsPlan, angle: f64, speed: f64) -> String {
    let geometry = step_geometry(plan, angle, speed);
    let [sx, sy] = geometry.start;
    let [ex, ey] = geometry.end;
    let run = &plan.run;
    let mut script = format!(
        "SAVE_GCODE_STATE NAME=wobblescope\nG90\nG1 X{sx:.3} Y{sy:.3} Z{} F{}\n",
        run.z_height,
        feed(run.travel_speed)
    );
    for _ in 0..geometry.repeats {
        script.push_str(&format!(
            "G1 X{ex:.3} Y{ey:.3} F{f}\nG1 X{sx:.3} Y{sy:.3} F{f}\n",
            f = feed(speed)
        ));
    }
    script.push_str("G4 P300\nM400\nRESTORE_GCODE_STATE NAME=wobblescope");
    script
}

/// The time one step at `requested` mm/s is expected to take, with its positioning move and
/// pause, in seconds.
fn estimated_step_seconds(run: &VibrationsRun, requested: f64) -> f64 {
    // Klipper caps every move at max_velocity, so a faster step takes longer.
    let cap = run.max_velocity.unwrap_or(f64::INFINITY);
    let speed = requested.min(cap);
    let travel_speed = run.travel_speed.min(cap);
    let length = run.size * length_multiplier(requested);
    let moves = f64::from(repeats(requested)) * 2.0;
    // Each move takes its length at speed plus the time to reach that speed. The positioning
    // move is taken at its full length.
    length / travel_speed + moves * (length / speed + speed / run.accel) + STEP_DWELL_SECONDS
}

#[must_use]
pub fn estimated_angle_seconds(plan: &VibrationsPlan) -> f64 {
    plan.speeds
        .iter()
        .map(|&requested| estimated_step_seconds(&plan.run, requested))
        .sum()
}

#[must_use]
pub fn slowest_step_seconds(plan: &VibrationsPlan) -> f64 {
    plan.speeds
        .iter()
        .map(|&requested| estimated_step_seconds(&plan.run, requested))
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moonraker::settings::{Limits, parse_settings};

    fn printer(kinematics: Option<&str>) -> PrinterSettings {
        let dir = format!("{}/tests/data/moonraker", env!("CARGO_MANIFEST_DIR"));
        let read = |name: &str| {
            serde_json::from_str(&std::fs::read_to_string(format!("{dir}/{name}")).unwrap())
                .unwrap()
        };
        let mut printer =
            parse_settings(&read("real_settings.json"), &read("real_printer_info.json")).unwrap();
        printer.machine.kinematics = kinematics.map(str::to_owned);
        printer.input_shaper_configured = true;
        let travel = Limits {
            min: 0.0,
            max: 250.0,
        };
        printer.axis_limits = Some(crate::moonraker::settings::AxisLimits {
            x: travel,
            y: travel,
            z: travel,
        });
        printer
    }

    fn request() -> VibrationsRequest {
        VibrationsRequest {
            chips: vec!["adxl345".to_owned()],
            size: None,
            z_height: None,
            min_speed: None,
            max_speed: None,
            speed_increment: None,
            accel: None,
            travel_speed: None,
            angular_step: None,
        }
    }

    fn planned(request: &VibrationsRequest) -> Result<TestPlan, RequestError> {
        plan_vibrations(request, &printer(Some("corexy")), 600, &[Some(3200.0)])
    }

    fn vibrations(request: &VibrationsRequest) -> VibrationsPlan {
        planned(request).unwrap().vibrations.unwrap()
    }

    fn refused(request: &VibrationsRequest) -> RequestError {
        planned(request).unwrap_err()
    }

    #[test]
    fn only_a_vibrations_plan_carries_vibrations() {
        use crate::capture::axes_map::{AxesMapRequest, ChipChoice, plan_axes_map};
        use crate::capture::request::{Overrides, TestRequest, plan};
        use wobblescope_core::axes_map::Mounting;
        use wobblescope_core::fit::Axis;

        let printer = printer(Some("corexy"));
        let vibrations_plan = planned(&request()).unwrap();
        let axes_map_plan = plan_axes_map(
            &AxesMapRequest {
                chips: vec![ChipChoice {
                    chip: "adxl345".to_owned(),
                    mounting: Mounting::Toolhead,
                }],
                z_height: None,
                speed: None,
                accel: None,
                travel_speed: None,
            },
            &printer,
            600,
        )
        .unwrap();
        let sweep_plan = plan(
            &TestRequest {
                chips: vec!["adxl345".to_owned()],
                kind: TestKind::Axes,
                axes: Some(vec![Axis::X]),
                mode: Mode::Sweep,
                overrides: Overrides::default(),
                points: None,
            },
            &printer,
            600,
        )
        .unwrap();
        for plan in [&vibrations_plan, &axes_map_plan, &sweep_plan] {
            assert_eq!(
                plan.vibrations.is_some(),
                plan.kind == TestKind::Vibrations,
                "{:?}",
                plan.kind
            );
        }
        assert_eq!(vibrations_plan.kind, TestKind::Vibrations);
        assert_eq!(axes_map_plan.kind, TestKind::AxesMap);
        assert_eq!(sweep_plan.kind, TestKind::Axes);
    }

    #[test]
    fn defaults_are_shaketunes() {
        let plan = vibrations(&request());
        let run = &plan.run;
        assert_eq!(
            (run.size, run.z_height, run.max_speed, run.speed_increment),
            (100.0, 20.0, 200.0, 2.0)
        );
        assert_eq!(
            (run.accel, run.travel_speed, run.min_speed),
            (3000.0, 120.0, 2.0)
        );
        assert_eq!(plan.speeds[0], 2.0);
    }

    #[test]
    fn start_speed_defaults_to_two() {
        let plan = vibrations(&request());
        assert_eq!((plan.run.min_speed, plan.speeds[0]), (2.0, 2.0));
    }

    #[test]
    fn a_custom_start_speed_starts_the_list_and_is_recorded() {
        let mut r = request();
        r.min_speed = Some(20.0);
        r.speed_increment = Some(5.0);
        let plan = vibrations(&r);
        assert_eq!(plan.run.min_speed, 20.0);
        assert_eq!(&plan.speeds[..3], &[20.0, 25.0, 30.0]);
    }

    #[test]
    fn a_start_speed_below_two_is_refused() {
        let mut r = request();
        r.min_speed = Some(1.5);
        assert_eq!(refused(&r).field, "min_speed");
    }

    #[test]
    fn a_start_speed_not_below_the_end_speed_is_refused() {
        let mut r = request();
        r.min_speed = Some(200.0);
        assert_eq!(refused(&r).field, "min_speed");
        r.min_speed = Some(250.0);
        assert_eq!(refused(&r).field, "min_speed");
        r.min_speed = Some(f64::NAN);
        assert_eq!(refused(&r).field, "min_speed");
    }

    #[test]
    fn the_last_speed_is_the_end_speed() {
        let mut r = request();
        r.speed_increment = Some(5.0);
        let speeds = vibrations(&r).speeds;
        assert_eq!(speeds.len(), 41);
        assert_eq!(&speeds[..3], &[2.0, 7.0, 12.0]);
        assert_eq!(&speeds[39..], &[197.0, 200.0]);
    }

    #[test]
    fn an_end_speed_on_a_step_is_listed_once() {
        let mut r = request();
        r.speed_increment = Some(2.0);
        let speeds = vibrations(&r).speeds;
        assert_eq!(speeds.len(), 100);
        assert_eq!(&speeds[97..], &[196.0, 198.0, 200.0]);
    }

    #[test]
    fn a_planned_run_ends_at_the_end_speed() {
        let mut r = request();
        r.speed_increment = Some(5.0);
        assert_eq!(vibrations(&r).speeds.last(), Some(&200.0));
    }

    #[test]
    fn segment_length_multiplier_below_100_is_one_fifth_plus_four_fifths() {
        assert!((length_multiplier(50.0) - 0.6).abs() < 1e-12);
        assert_eq!(length_multiplier(100.0), 1.0);
        assert_eq!(length_multiplier(180.0), 1.0);
    }

    #[test]
    fn repeats_are_one_two_three_by_150_and_250() {
        assert_eq!(repeats(149.0), 1);
        assert_eq!(repeats(150.0), 2);
        assert_eq!(repeats(249.0), 2);
        assert_eq!(repeats(250.0), 3);
    }

    #[test]
    fn size_over_max_speed_below_a_quarter_is_refused() {
        let mut r = request();
        r.size = Some(50.0);
        r.max_speed = Some(500.0);
        assert_eq!(refused(&r).field, "max_speed");
        r.max_speed = Some(200.0);
        assert!(planned(&r).is_ok());
    }

    #[test]
    fn a_step_wider_than_the_range_still_gives_two_speeds() {
        let mut r = request();
        r.max_speed = Some(10.0);
        r.speed_increment = Some(50.0);
        assert_eq!(vibrations(&r).speeds, vec![2.0, 10.0]);
    }

    #[test]
    fn a_printer_without_an_input_shaper_section_is_refused() {
        let mut printer = printer(Some("corexy"));
        printer.input_shaper_configured = false;
        let err = plan_vibrations(&request(), &printer, 600, &[Some(3200.0)]).unwrap_err();
        assert_eq!(err.field, "input_shaper");
        assert!(err.to_string().contains("shaper calibration"), "{err}");
    }

    #[test]
    fn corexy_uses_45_and_135_cartesian_uses_0_and_90() {
        assert_eq!(vibrations(&request()).angles, [45.0, 135.0]);
        let plan = plan_vibrations(&request(), &printer(Some("cartesian")), 600, &[None]).unwrap();
        assert_eq!(plan.vibrations.unwrap().angles, [0.0, 90.0]);
        assert_eq!(plan.segments[0].axis, Direction::X);
        assert_eq!(plan.segments[1].axis, Direction::Y);
    }

    #[test]
    fn unknown_or_missing_kinematics_is_refused() {
        for kinematics in [Some("delta"), None] {
            let err = plan_vibrations(&request(), &printer(kinematics), 600, &[None]).unwrap_err();
            assert_eq!(err.field, "kinematics");
        }
    }

    #[test]
    fn extent_outside_travel_is_refused() {
        let mut r = request();
        r.size = Some(260.0);
        let err = refused(&r);
        assert_eq!(err.field, "size");
        let mut r = request();
        r.z_height = Some(300.0);
        assert_eq!(refused(&r).field, "z_height");
        // A square that fits on X alone does not rescue one that overruns Y.
        let mut printer = printer(Some("corexy"));
        let limits = printer.axis_limits.as_mut().unwrap();
        limits.y = Limits {
            min: 100.0,
            max: 150.0,
        };
        let err = plan_vibrations(&request(), &printer, 600, &[None]).unwrap_err();
        assert_eq!(err.field, "size");
    }

    #[test]
    fn angle_that_cannot_fit_the_sample_limit_is_refused() {
        let mut r = request();
        r.speed_increment = Some(1.0);
        let err = plan_vibrations(
            &r,
            &printer(Some("corexy")),
            600,
            &[Some(3200.0), Some(100_000.0)],
        )
        .unwrap_err();
        assert_eq!(err.field, "max_speed");
        assert!(plan_vibrations(&r, &printer(Some("corexy")), 600, &[None]).is_ok());
    }

    #[test]
    fn run_metadata_carries_kinematics_and_limits() {
        let plan = vibrations(&request());
        assert_eq!(plan.run.kinematics, "corexy");
        assert_eq!(
            plan.run.max_velocity,
            printer(Some("corexy")).machine.max_velocity
        );
        assert_eq!(plan.mid, [125.0, 125.0]);
    }

    #[test]
    fn step_geometry_and_script_follow_shaketune() {
        let plan = vibrations(&request());
        let g = step_geometry(&plan, 0.0, 50.0);
        assert!((g.start[0] - (125.0 - 30.0)).abs() < 1e-9 && (g.end[0] - 155.0).abs() < 1e-9);
        assert_eq!(g.repeats, 1);
        let script = step_script(&plan, 0.0, 50.0);
        assert_eq!(
            script,
            "SAVE_GCODE_STATE NAME=wobblescope\nG90\nG1 X95.000 Y125.000 Z20 F7200\n\
             G1 X155.000 Y125.000 F3000\nG1 X95.000 Y125.000 F3000\nG4 P300\nM400\n\
             RESTORE_GCODE_STATE NAME=wobblescope"
        );
    }

    #[test]
    fn estimate_caps_speed_and_travel_at_max_velocity() {
        let mut plan = vibrations(&request());
        plan.speeds = vec![200.0];
        plan.run.size = 100.0;
        plan.run.accel = 1000.0;
        plan.run.travel_speed = 400.0;
        plan.run.max_velocity = None;
        // Travel 100/400, four moves at (100/200 + 200/1000), dwell 0.3.
        let free = estimated_angle_seconds(&plan);
        assert!((free - 3.35).abs() < 1e-9, "{free}");
        plan.run.max_velocity = Some(100.0);
        // Travel 100/100, four moves at (100/100 + 100/1000), dwell 0.3.
        let capped = estimated_angle_seconds(&plan);
        assert!((capped - 5.7).abs() < 1e-9, "{capped}");
        assert!(capped > free);
    }

    #[test]
    fn a_low_max_velocity_can_refuse_a_run_that_fits_without_it() {
        let uncapped = printer(Some("corexy"));
        let mut low = printer(Some("corexy"));
        low.machine.max_velocity = Some(20.0);
        let r = request();
        let mut plan = vibrations(&r);
        plan.run.max_velocity = None;
        let free = estimated_angle_seconds(&plan);
        plan.run.max_velocity = Some(20.0);
        let capped = estimated_angle_seconds(&plan);
        assert!(capped > free * 1.5);
        // A rate between the two limits: the free estimate passes and the capped one fails.
        let rate = MAX_SAMPLES as f64 / (SAMPLE_MARGIN * (free * capped).sqrt());
        let mut uncapped = uncapped;
        uncapped.machine.max_velocity = None;
        assert!(plan_vibrations(&r, &uncapped, 600, &[Some(rate)]).is_ok());
        let err = plan_vibrations(&r, &low, 600, &[Some(rate)]).unwrap_err();
        assert_eq!(err.field, "max_speed");
    }

    fn stepped(step: f64) -> VibrationsRequest {
        VibrationsRequest {
            angular_step: Some(step),
            ..request()
        }
    }

    #[test]
    fn angular_step_45_plans_four_angles_motor_first() {
        let plan = planned(&stepped(45.0)).unwrap();
        let vibrations = plan.vibrations.unwrap();
        assert_eq!(vibrations.angles, [45.0, 135.0, 0.0, 90.0]);
        assert_eq!(vibrations.run.angular_step, Some(45.0));
        let axes: Vec<Direction> = plan.segments.iter().map(|s| s.axis).collect();
        assert_eq!(
            axes,
            [
                Direction::BELT_B,
                Direction::BELT_A,
                Direction::X,
                Direction::Y
            ]
        );
    }

    #[test]
    fn corexy_45_plans_45_135_0_90() {
        assert_eq!(vibrations(&stepped(45.0)).angles, [45.0, 135.0, 0.0, 90.0]);
        let plan =
            plan_vibrations(&stepped(45.0), &printer(Some("cartesian")), 600, &[None]).unwrap();
        assert_eq!(plan.vibrations.unwrap().angles, [0.0, 90.0, 45.0, 135.0]);
    }

    #[test]
    fn step_22_5_and_15_plan_eight_and_twelve() {
        let eight = vibrations(&stepped(22.5));
        assert_eq!(eight.angles.len(), 8);
        assert_eq!(&eight.angles[..2], &[45.0, 135.0]);
        assert_eq!(vibrations(&stepped(15.0)).angles.len(), 12);
    }

    #[test]
    fn an_unaccepted_step_is_refused_naming_angular_step() {
        for step in [30.0, 0.0, -15.0, f64::NAN] {
            assert_eq!(refused(&stepped(step)).field, "angular_step", "{step}");
        }
    }

    #[test]
    fn no_step_plans_the_motor_pair_and_writes_the_version_it_writes_today() {
        use crate::capture::meta::initial;
        let plan = planned(&request()).unwrap();
        assert_eq!(plan.vibrations.as_ref().unwrap().angles, [45.0, 135.0]);
        assert_eq!(plan.segments.len(), 2);
        let meta = initial(&plan, "vibrations-20261101T121400Z", "now", &[], None);
        assert_eq!(meta.format_version, 1);
        assert!(meta.vibrations_run.is_none());
    }

    #[test]
    fn the_sample_limit_uses_the_fastest_chip_rate_for_every_angle() {
        let r = stepped(15.0);
        let printer = printer(Some("corexy"));
        let seconds = estimated_angle_seconds(&vibrations(&r));
        // The rate at which one angle just fits the sample file.
        let limit = MAX_SAMPLES as f64 / (SAMPLE_MARGIN * seconds);
        let (slow, fast) = (limit * 0.5, limit * 2.0);
        assert!(plan_vibrations(&r, &printer, 600, &[Some(slow)]).is_ok());
        let both = plan_vibrations(&r, &printer, 600, &[Some(slow), Some(fast)]);
        assert_eq!(both.unwrap_err().field, "max_speed");
    }

    #[test]
    fn estimated_step_seconds_matches_hand_computed_values() {
        let mut plan = vibrations(&request());
        plan.run.size = 100.0;
        plan.run.accel = 1000.0;
        plan.run.travel_speed = 400.0;
        plan.run.max_velocity = Some(300.0);
        // Speed 50: length 100 * 0.6 = 60, one repeat = two moves, no cap.
        let at_50 = 60.0 / 300.0_f64.min(400.0) + 2.0 * (60.0 / 50.0 + 50.0 / 1000.0) + 0.3;
        // Speed 150: length 100, two repeats = four moves.
        let at_150 = 100.0 / 300.0 + 4.0 * (100.0 / 150.0 + 150.0 / 1000.0) + 0.3;
        // Speed 250: length 100, three repeats = six moves, capped at 300 so unaffected.
        let at_250 = 100.0 / 300.0 + 6.0 * (100.0 / 250.0 + 250.0 / 1000.0) + 0.3;
        // Speed 400 is above the cap of 300: it runs at 300.
        let at_400 = 100.0 / 300.0 + 6.0 * (100.0 / 300.0 + 300.0 / 1000.0) + 0.3;
        for (speed, want) in [
            (50.0, at_50),
            (150.0, at_150),
            (250.0, at_250),
            (400.0, at_400),
        ] {
            let got = estimated_step_seconds(&plan.run, speed);
            assert!((got - want).abs() < 1e-9, "{speed}: {got} vs {want}");
        }
    }

    fn journal_of(step: f64) -> (wobblescope_session::meta::SessionMeta, VibrationsPlan) {
        use crate::capture::meta::initial;
        let plan = planned(&stepped(step)).unwrap();
        let meta = initial(&plan, "vibrations-20261101T121400Z", "now", &[], None);
        (meta, plan.vibrations.unwrap())
    }

    #[test]
    fn the_journal_of_a_step_run_is_valid_and_version_3_from_creation_and_after_each_stored_segment()
     {
        use wobblescope_session::meta::SegmentMeta;
        let (mut meta, plan) = journal_of(22.5);
        assert_eq!(meta.format_version, 3);
        meta.validate().unwrap();
        for &angle in &plan.angles {
            meta.segments.push(SegmentMeta {
                axis: Direction::from_degrees(angle),
                point: 0,
                window: None,
                files: Vec::new(),
                steps: Vec::new(),
            });
            meta.format_version = meta.required_version();
            assert_eq!(meta.format_version, 3);
            meta.validate().unwrap();
        }
    }

    #[test]
    fn recovery_before_any_segment_keeps_the_plan_and_version_3() {
        let (meta, plan) = journal_of(15.0);
        let read =
            wobblescope_session::meta::SessionMeta::from_json(&meta.to_json().unwrap()).unwrap();
        assert_eq!(read.format_version, 3);
        assert!(read.segments.is_empty());
        let run = read.vibrations_run.unwrap();
        assert_eq!(run.angular_step, Some(15.0));
        assert_eq!(run.planned_angles().unwrap(), plan.angles);
    }

    #[test]
    fn step_script_feed_has_no_float_noise() {
        let plan = vibrations(&request());
        let script = step_script(&plan, 0.0, 2.2);
        assert!(script.contains("F132\n"), "{script}");
        assert!(!script.contains("00000"), "{script}");
        let script = step_script(&plan, 0.0, 33.3335);
        assert!(script.contains("F2000.01\n"), "{script}");
    }
}
