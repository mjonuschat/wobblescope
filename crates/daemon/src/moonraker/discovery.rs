//! Parsers for Moonraker's answers: the file roots, and the printer's raw config.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::{Map, Value};

use super::chips::{Chip, Section, SkippedSection, classify};

/// Why Moonraker's file roots could not give the config root.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RootsError {
    /// The answer is not an array of roots.
    #[error("Moonraker's file roots answer is not a list of roots")]
    NotAList,
    /// No root is named `config`.
    #[error("Moonraker reports no config root")]
    NoConfigRoot,
    /// The config root has no usable path.
    #[error("Moonraker's config root has no path")]
    NoPath,
}

/// Why the config query's answer cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigfileError {
    /// The answer has no `status.configfile.config` object.
    #[error("the config query's answer has no configfile.config")]
    NoConfig,
}

/// The chips a test would use by default on each axis.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Defaults {
    /// Section names for the X axis.
    pub x: Vec<String>,
    /// Section names for the Y axis.
    pub y: Vec<String>,
}

/// What the printer's config says about accelerometers.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChipList {
    /// The usable accelerometer sections.
    pub chips: Vec<Chip>,
    /// The accelerometer sections the daemon cannot use yet.
    pub skipped: Vec<SkippedSection>,
    /// The default chips per axis.
    pub defaults: Defaults,
}

/// The path of the `config` root in the `result` of `server.files.roots`.
///
/// # Errors
/// A [`RootsError`] when the answer is not a list, has no `config` root or that root has no path.
pub fn parse_config_root(result: &Value) -> Result<PathBuf, RootsError> {
    let roots = result.as_array().ok_or(RootsError::NotAList)?;
    let root = roots
        .iter()
        .find(|r| r.get("name").and_then(Value::as_str) == Some("config"))
        .ok_or(RootsError::NoConfigRoot)?;
    root.get("path")
        .and_then(Value::as_str)
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .ok_or(RootsError::NoPath)
}

/// Chips, skipped sections and defaults from the `result` of a `printer.objects.query` for
/// `configfile.config`.
///
/// # Errors
/// [`ConfigfileError::NoConfig`] when the answer lacks `status.configfile.config`.
pub fn parse_configfile(result: &Value) -> Result<ChipList, ConfigfileError> {
    let config = result
        .get("status")
        .and_then(|s| s.get("configfile"))
        .and_then(|c| c.get("config"))
        .and_then(Value::as_object)
        .ok_or(ConfigfileError::NoConfig)?;
    let mut list = ChipList::default();
    for (name, options) in config {
        let Some(options) = options.as_object() else {
            continue;
        };
        match classify(name, options) {
            Section::Chip(chip) => list.chips.push(chip),
            Section::Skipped(skipped) => list.skipped.push(skipped),
            Section::Ignored => {}
        }
    }
    let tester = config.get("resonance_tester").and_then(Value::as_object);
    list.defaults = tester.map_or_else(Defaults::default, |t| defaults(t, &list.chips));
    Ok(list)
}

/// The non-empty, trimmed names in a comma-separated option.
fn names(text: Option<&str>) -> Vec<&str> {
    text.map(|t| {
        t.split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// The default chips, by Kalico's `accel_chips`, then `accel_chip_x` and `accel_chip_y`, then
/// `accel_chip`. Names that are not listed chips are left out.
fn defaults(tester: &Map<String, Value>, chips: &[Chip]) -> Defaults {
    let option = |key: &str| tester.get(key).and_then(Value::as_str);
    let keep = |names: Vec<&str>| -> Vec<String> {
        names
            .into_iter()
            .filter(|n| chips.iter().any(|c| c.section == *n))
            .map(str::to_owned)
            .collect()
    };
    if let Some(list) = option("accel_chips").filter(|t| !t.trim().is_empty()) {
        let both = keep(names(Some(list)));
        return Defaults {
            x: both.clone(),
            y: both,
        };
    }
    let (x, y) = (option("accel_chip_x"), option("accel_chip_y"));
    if x.is_some() || y.is_some() {
        return Defaults {
            x: keep(names(x)),
            y: keep(names(y)),
        };
    }
    let both = keep(names(option("accel_chip")));
    Defaults {
        x: both.clone(),
        y: both,
    }
}
