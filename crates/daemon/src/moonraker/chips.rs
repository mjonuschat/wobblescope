//! Which Klipper config sections are accelerometers, and what the daemon knows about each type.

use serde::Serialize;
use serde_json::{Map, Value};

/// The accelerometer types the daemon can capture from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ChipType {
    /// An ADXL345, plain or named.
    Adxl345,
    /// An LIS2DW, plain or named.
    Lis2dw,
    Beacon,
}

/// Whether a chip was seen delivering samples.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Availability {
    /// No check has run.
    Unchecked,
    /// The subscription succeeded and valid samples arrived.
    Available {
        /// The sample rate seen during the check, in Hz.
        observed_rate_hz: f64,
        /// When the check ran, as an ISO 8601 UTC time.
        checked_at: String,
    },
    /// The chip did not deliver, with the reason.
    Unavailable {
        /// What went wrong, in Klipper's words where it had any.
        reason: String,
    },
}

/// An accelerometer section of the Klipper config.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Chip {
    /// The section name as configured, for example `adxl345 nh36`.
    pub section: String,
    /// The type, from the section's first word.
    pub chip_type: ChipType,
    /// The key Klipper's subscription takes for this chip, when it has one.
    pub sensor: Option<String>,
    /// The Klipper endpoint that streams the chip's samples.
    pub dump_endpoint: String,
    /// The chip's nominal sample rate, when known.
    pub nominal_rate_hz: Option<f64>,
    /// The Klipper MCU the chip is wired to: `mcu` for the main board, else the name in front of
    /// the chip's `cs_pin` or its `i2c_mcu`. `None` for a chip that has its own device.
    pub mcu: Option<String>,
    /// The chip's `axes_map` as configured, without spaces, such as `x,z,-y`. `None` when the
    /// config does not set it, which means Klipper's default `x,y,z`.
    pub axes_map: Option<String>,
    /// Whether the chip was seen delivering samples.
    pub availability: Availability,
}

/// An accelerometer section the daemon cannot use yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SkippedSection {
    /// The section name as configured.
    pub section: String,
    /// Why it is skipped.
    pub reason: String,
}

/// What a config section is to the daemon.
#[derive(Debug, Clone, PartialEq)]
pub enum Section {
    /// A supported accelerometer.
    Chip(Chip),
    /// An accelerometer the daemon cannot use yet.
    Skipped(SkippedSection),
    /// Not an accelerometer.
    Ignored,
}

const ADXL345_DEFAULT_RATE_HZ: f64 = 3200.0;
const LIS2DW_RATE_HZ: f64 = 1600.0;

/// Classifies the config section `name` with its raw `options`.
#[must_use]
pub fn classify(name: &str, options: &Map<String, Value>) -> Section {
    let words: Vec<&str> = name.split_whitespace().collect();
    let Some(&kind) = words.first() else {
        return Section::Ignored;
    };
    let last = words.last().copied().unwrap_or(kind);
    let skip = |reason: &str| {
        Section::Skipped(SkippedSection {
            section: name.to_owned(),
            reason: reason.to_owned(),
        })
    };
    match kind {
        "adxl345" => Section::Chip(Chip {
            section: name.to_owned(),
            chip_type: ChipType::Adxl345,
            sensor: Some(last.to_owned()),
            dump_endpoint: "adxl345/dump_adxl345".to_owned(),
            nominal_rate_hz: match options.get("rate") {
                None => Some(ADXL345_DEFAULT_RATE_HZ),
                Some(v) => positive_number(v),
            },
            mcu: mcu_of(options),
            axes_map: axes_map_of(options),
            availability: Availability::Unchecked,
        }),
        "lis2dw" => Section::Chip(Chip {
            section: name.to_owned(),
            chip_type: ChipType::Lis2dw,
            sensor: Some(last.to_owned()),
            dump_endpoint: "lis2dw/dump_lis2dw".to_owned(),
            nominal_rate_hz: Some(LIS2DW_RATE_HZ),
            mcu: mcu_of(options),
            axes_map: axes_map_of(options),
            availability: Availability::Unchecked,
        }),
        "beacon" => classify_beacon(name, &words[1..], options),
        "lis3dh" => skip("lis3dh is not supported yet"),
        "mpu9250" => skip("mpu9250 is not supported yet"),
        "icm20948" => skip("icm20948 is not supported yet"),
        _ => Section::Ignored,
    }
}

/// A `[beacon]` or `[beacon sensor <name>]` section is a probe with an accelerometer. A `model`
/// section, alone or after the sensor name, holds a calibration model and is not a device.
/// `after` is the section name's words after `beacon`.
fn classify_beacon(name: &str, after: &[&str], options: &Map<String, Value>) -> Section {
    let (sensor, rest) = match after {
        ["sensor", sensor, rest @ ..] => (Some(*sensor), rest),
        rest => (None, rest),
    };
    match rest {
        [] => Section::Chip(Chip {
            section: name.to_owned(),
            chip_type: ChipType::Beacon,
            sensor: sensor.map(str::to_owned),
            dump_endpoint: "beacon/dump_accel".to_owned(),
            nominal_rate_hz: None,
            mcu: None,
            axes_map: option_without_spaces(options, "accel_axes_map"),
            availability: Availability::Unchecked,
        }),
        ["model", ..] => Section::Ignored,
        _ => Section::Skipped(SkippedSection {
            section: name.to_owned(),
            reason: "not a Beacon section the daemon knows".to_owned(),
        }),
    }
}

/// The MCU a chip is wired to. An SPI chip's `cs_pin` is `<mcu>:<pin>` when it is not on the main
/// board, an I2C chip names it in `i2c_mcu`, and anything else is the main MCU.
fn mcu_of(options: &Map<String, Value>) -> Option<String> {
    let text = |key: &str| options.get(key).and_then(Value::as_str).map(str::trim);
    let named = text("cs_pin")
        .and_then(|pin| pin.split_once(':'))
        .map(|(mcu, _)| mcu.trim())
        .or_else(|| text("i2c_mcu"))
        .filter(|mcu| !mcu.is_empty());
    Some(named.unwrap_or("mcu").to_owned())
}

/// The `axes_map` option with its spaces removed, when the config sets one.
fn axes_map_of(options: &Map<String, Value>) -> Option<String> {
    option_without_spaces(options, "axes_map")
}

fn option_without_spaces(options: &Map<String, Value>, key: &str) -> Option<String> {
    let text = options.get(key)?.as_str()?;
    let map: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    (!map.is_empty()).then_some(map)
}

/// A config value as a positive, finite number. Klipper's raw config holds strings.
fn positive_number(value: &Value) -> Option<f64> {
    let n = match value {
        Value::String(s) => s.trim().parse::<f64>().ok()?,
        Value::Number(n) => n.as_f64()?,
        _ => return None,
    };
    (n.is_finite() && n > 0.0).then_some(n)
}
