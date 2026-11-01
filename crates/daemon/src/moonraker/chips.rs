//! Which Klipper config sections are accelerometers, and what the daemon knows about each type.

use serde::Serialize;
use serde_json::{Map, Value};

/// The accelerometer types the daemon can capture from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChipType {
    /// An ADXL345, plain or named.
    Adxl345,
    /// An LIS2DW, plain or named.
    Lis2dw,
    /// The plain Beacon probe's accelerometer.
    Beacon,
}

/// Whether a chip was seen delivering samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Availability {
    /// No check has run.
    Unchecked,
}

/// An accelerometer section of the Klipper config.
#[derive(Debug, Clone, PartialEq, Serialize)]
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
    /// Whether the chip was seen delivering samples.
    pub availability: Availability,
}

/// An accelerometer section the daemon cannot use yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
    let named = words.len() > 1;
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
            availability: Availability::Unchecked,
        }),
        "lis2dw" => Section::Chip(Chip {
            section: name.to_owned(),
            chip_type: ChipType::Lis2dw,
            sensor: Some(last.to_owned()),
            dump_endpoint: "lis2dw/dump_lis2dw".to_owned(),
            nominal_rate_hz: Some(LIS2DW_RATE_HZ),
            availability: Availability::Unchecked,
        }),
        "beacon" if !named => Section::Chip(Chip {
            section: name.to_owned(),
            chip_type: ChipType::Beacon,
            sensor: None,
            dump_endpoint: "beacon/dump_accel".to_owned(),
            nominal_rate_hz: None,
            availability: Availability::Unchecked,
        }),
        // `[beacon model <name>]` holds a calibration model, not a device.
        "beacon" if words[1] == "model" => Section::Ignored,
        "beacon" => skip("named Beacon devices are not supported yet"),
        "lis3dh" => skip("lis3dh is not supported yet"),
        "mpu9250" => skip("mpu9250 is not supported yet"),
        "icm20948" => skip("icm20948 is not supported yet"),
        _ => Section::Ignored,
    }
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
