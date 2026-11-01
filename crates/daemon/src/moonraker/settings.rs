//! The printer's `[resonance_tester]` settings as Klipper will use them, and the toolhead's
//! limits, parsed from Moonraker's answers.

use serde::Serialize;
use serde_json::Value;
use wobblescope_session::meta::Mode;

/// The z a test point starts at when the toolhead allows it, in mm.
const DEFAULT_Z: f64 = 20.0;

/// Which Klipper fork answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Variant {
    /// Mainline Klipper.
    Mainline,
    /// Kalico.
    Kalico,
    /// Something else; only settings found in its config are shown.
    Unknown,
}

/// One setting: what Klipper will use, and what a test may do with it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Setting {
    /// The effective value, or `None` when the daemon cannot read it.
    pub value: Option<f64>,
    /// Whether a test may override it.
    pub overridable: bool,
    /// Whether a test cannot run without a value, so an unreadable one must be overridden.
    pub needed: bool,
}

/// The seven settings a test may override. Names follow `TEST_RESONANCES` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResonanceSettings {
    /// `min_freq`.
    pub freq_start: Setting,
    /// `max_freq`.
    pub freq_end: Setting,
    /// `hz_per_sec`.
    pub hz_per_sec: Setting,
    /// `accel_per_hz`.
    pub accel_per_hz: Setting,
    /// `sweeping_accel`.
    pub sweeping_accel: Setting,
    /// `sweeping_period`.
    pub sweeping_period: Setting,
    /// `move_speed`, the speed of the move to a test point.
    pub move_speed: Setting,
}

/// One axis' travel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Limits {
    /// The lowest position, in mm.
    pub min: f64,
    /// The highest position, in mm.
    pub max: f64,
}

/// The toolhead's travel on each axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AxisLimits {
    /// X travel.
    pub x: Limits,
    /// Y travel.
    pub y: Limits,
    /// Z travel.
    pub z: Limits,
}

/// What `GET /api/settings` answers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PrinterSettings {
    /// The Klipper fork.
    pub variant: Variant,
    /// `Sweep` when the effective `sweeping_period` is above 0, else `Pulse`.
    pub default_mode: Mode,
    /// The effective settings.
    pub settings: ResonanceSettings,
    /// The toolhead's travel, when Klipper reported it.
    pub axis_limits: Option<AxisLimits>,
    /// A test point in the middle of X and Y at a low Z, when the limits are known.
    pub default_point: Option<[f64; 3]>,
}

/// Why an answer could not be turned into settings.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    /// The answer has no `configfile.settings`.
    #[error("the settings query's answer has no configfile.settings")]
    NoSettings,
}

/// The fork named by `printer.info`'s `app`. A missing `app` is mainline Klipper.
#[must_use]
pub fn parse_variant(info: &Value) -> Variant {
    match info.get("app").and_then(Value::as_str) {
        None | Some("Klipper") => Variant::Mainline,
        Some("Kalico") => Variant::Kalico,
        Some(_) => Variant::Unknown,
    }
}

/// Builds the settings from the answers to the `configfile.settings`/`toolhead` query and
/// `printer.info`.
///
/// # Errors
/// [`SettingsError::NoSettings`] when the query's answer has no `configfile.settings`.
pub fn parse_settings(query: &Value, info: &Value) -> Result<PrinterSettings, SettingsError> {
    let status = query.get("status");
    let all = status
        .and_then(|s| s.get("configfile"))
        .and_then(|c| c.get("settings"))
        .and_then(Value::as_object)
        .ok_or(SettingsError::NoSettings)?;
    let variant = parse_variant(info);
    let tester = all.get("resonance_tester").and_then(Value::as_object);
    let read = |key: &str, needed: bool| {
        let value = tester.and_then(|t| t.get(key)).and_then(number);
        Setting {
            value,
            // Without a fork the daemon knows, only what the config shows can be overridden.
            overridable: variant != Variant::Unknown || value.is_some(),
            needed,
        }
    };
    let settings = ResonanceSettings {
        freq_start: read("min_freq", true),
        freq_end: read("max_freq", true),
        hz_per_sec: read("hz_per_sec", true),
        accel_per_hz: read("accel_per_hz", true),
        sweeping_accel: read("sweeping_accel", false),
        sweeping_period: read("sweeping_period", false),
        move_speed: read("move_speed", false),
    };
    let default_mode = if settings.sweeping_period.value.is_some_and(|p| p > 0.0) {
        Mode::Sweep
    } else {
        Mode::Pulse
    };
    let axis_limits = status
        .and_then(|s| s.get("toolhead"))
        .and_then(parse_limits);
    let default_point = axis_limits.map(|l| {
        [
            (l.x.min + l.x.max) / 2.0,
            (l.y.min + l.y.max) / 2.0,
            DEFAULT_Z.clamp(l.z.min, l.z.max),
        ]
    });
    Ok(PrinterSettings {
        variant,
        default_mode,
        settings,
        axis_limits,
        default_point,
    })
}

/// A number, or text that holds one; anything else, and anything not finite, is unreadable.
fn number(value: &Value) -> Option<f64> {
    let n = match value {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

/// The toolhead's `axis_minimum` and `axis_maximum`, each a list `[x, y, z, e]`.
fn parse_limits(toolhead: &Value) -> Option<AxisLimits> {
    let axis = |key: &str, i: usize| toolhead.get(key)?.as_array()?.get(i).and_then(number);
    let limits = |i| {
        let (min, max) = (axis("axis_minimum", i)?, axis("axis_maximum", i)?);
        (min <= max).then_some(Limits { min, max })
    };
    Some(AxisLimits {
        x: limits(0)?,
        y: limits(1)?,
        z: limits(2)?,
    })
}
