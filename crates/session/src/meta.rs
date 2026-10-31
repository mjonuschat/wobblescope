//! Session metadata: the `meta.json` shape and the rules a valid session obeys.

use serde::{Deserialize, Serialize};
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::fit::Axis;

use crate::SessionError;
use crate::codec::MAX_SAMPLES;

/// Version of the metadata shape this crate writes and reads.
pub const FORMAT_VERSION: u32 = 1;

const MAX_ID_LEN: usize = 64;

/// Where a session came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Source {
    /// Recorded by the daemon during a test.
    Capture,
    /// Converted from a Klipper raw CSV.
    CsvImport {
        /// Name of the file that was imported, when known.
        #[serde(default)]
        original_file_name: Option<String>,
    },
}

/// The clock sample timestamps and segment windows are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum TimeDomain {
    /// Klipper's toolhead `print_time`, in seconds.
    KlipperPrintTime,
}

/// Test mode, as in Klipper's `TEST_RESONANCES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Mode {
    /// `SWEEPING_PERIOD=0`: the toolhead vibrates in place.
    Pulse,
    /// `SWEEPING_PERIOD` above 0: the toolhead also moves back and forth.
    Sweep,
}

/// The sweep settings a test actually ran with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SweepSettings {
    /// Start frequency in Hz.
    pub freq_start: f64,
    /// End frequency in Hz.
    pub freq_end: f64,
    /// Frequency ramp in Hz per second.
    pub hz_per_sec: f64,
    /// Acceleration per Hz in mm/s².
    pub accel_per_hz: f64,
    /// Acceleration of the sweeping movement in mm/s², in sweep mode.
    #[serde(default)]
    pub sweeping_accel: Option<f64>,
    /// Period of the sweeping movement in seconds.
    #[serde(default)]
    pub sweeping_period: Option<f64>,
}

/// Printer identity recorded with a capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PrinterInfo {
    /// Printer name, when known.
    #[serde(default)]
    pub name: Option<String>,
    /// Klipper (or Kalico) version string.
    #[serde(default)]
    pub klipper_version: Option<String>,
    /// Firmware versions reported by the MCUs, one string per MCU.
    #[serde(default)]
    pub mcu_firmware: Vec<String>,
}

/// How a session ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Outcome {
    /// Every segment was captured.
    Complete,
    /// Sealed with part of the data missing.
    Incomplete {
        /// What is missing.
        reason: String,
    },
    /// The test failed; the data kept is partial.
    Failed {
        /// Why it failed.
        reason: String,
    },
}

/// A test point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProbePoint {
    /// Position in machine coordinates (mm); unknown for imports without one.
    #[serde(default)]
    pub position: Option<[f64; 3]>,
}

/// One accelerometer in the session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChipMeta {
    /// Klipper config section name, such as `adxl345 nh36`; also what `CHIPS=` takes.
    pub name: String,
    /// Sensor type, such as `ADXL345`.
    #[serde(default)]
    pub chip_type: Option<String>,
    /// The type's nominal sample rate in Hz.
    #[serde(default)]
    pub nominal_rate_hz: Option<f64>,
    /// Sample rate measured from the recording in Hz.
    #[serde(default)]
    pub observed_rate_hz: Option<f64>,
    /// The chip's `axes_map`, as configured.
    #[serde(default)]
    pub axes_map: Option<String>,
    /// Where the chip is mounted.
    #[serde(default)]
    pub mounting: Option<String>,
    /// Capture health of this chip for the test.
    pub health: Health,
    /// Human-readable notes behind the health value.
    #[serde(default)]
    pub health_notes: Vec<String>,
}

/// A window of sample timestamps, start inclusive and end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TimeWindow {
    /// Window start in seconds.
    pub start: f64,
    /// Window end in seconds.
    pub end: f64,
}

/// One chip's recording of one segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SegmentFile {
    /// Position of the chip in [`SessionMeta::chips`].
    pub chip: u32,
    /// File name inside the session directory; always [`sample_file_name`] of its position.
    pub sample_file: String,
    /// Number of samples in the file.
    pub sample_count: u32,
    /// Whether the recording covers the whole segment.
    pub coverage: Coverage,
}

/// One (axis, probe point) measurement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SegmentMeta {
    /// Test axis.
    pub axis: Axis,
    /// Position of the point in [`SessionMeta::probe_points`].
    pub point: u32,
    /// Sample timestamps that belong to the segment; `None` keeps every sample.
    #[serde(default)]
    pub window: Option<TimeWindow>,
    /// One entry per chip that recorded this segment.
    pub files: Vec<SegmentFile>,
}

/// The contents of `meta.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SessionMeta {
    /// Version of this shape; see [`FORMAT_VERSION`].
    pub format_version: u32,
    /// Session id, also the directory name.
    pub id: String,
    /// Creation time as an RFC 3339 UTC string, supplied by the caller.
    pub created_at: String,
    /// Where the session came from.
    pub source: Source,
    /// Clock of all timestamps.
    pub time_domain: TimeDomain,
    /// Test mode, absent for imports.
    #[serde(default)]
    pub mode: Option<Mode>,
    /// Settings the test ran with, absent for imports.
    #[serde(default)]
    pub settings: Option<SweepSettings>,
    /// Printer identity, when known.
    #[serde(default)]
    pub printer: Option<PrinterInfo>,
    /// How the session ended.
    pub outcome: Outcome,
    /// Free-text notes.
    #[serde(default)]
    pub notes: Vec<String>,
    /// The test points.
    #[serde(default)]
    pub probe_points: Vec<ProbePoint>,
    /// The accelerometers.
    pub chips: Vec<ChipMeta>,
    /// The segments, in the order they ran.
    pub segments: Vec<SegmentMeta>,
}

fn invalid(field: impl Into<String>, reason: impl Into<String>) -> SessionError {
    SessionError::Invalid {
        field: field.into(),
        reason: reason.into(),
    }
}

/// Checks a session id: 1 to 64 characters of `A-Z a-z 0-9 _ -`, Klipper's rule for `NAME`.
///
/// # Errors
/// [`SessionError::Invalid`] naming `id`.
pub fn validate_session_id(id: &str) -> Result<(), SessionError> {
    let ok = !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok {
        Ok(())
    } else {
        Err(invalid("id", "must be 1-64 characters of A-Z a-z 0-9 _ -"))
    }
}

/// The only valid file name for the recording of `chip` in `segment` (both list positions).
pub fn sample_file_name(chip: usize, segment: usize) -> String {
    format!("c{chip}-s{segment}.zst")
}

fn positive_finite(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

#[derive(Deserialize)]
struct VersionProbe {
    format_version: u32,
}

impl SessionMeta {
    /// Parses and validates `meta.json`. The version is read first, so a newer format fails
    /// with [`SessionError::UnsupportedFormatVersion`] even when its shape no longer parses.
    ///
    /// # Errors
    /// A [`SessionError`] for bad JSON, a newer version or a rule violation.
    pub fn from_json(json: &str) -> Result<Self, SessionError> {
        let probe: VersionProbe = serde_json::from_str(json)?;
        if probe.format_version != FORMAT_VERSION {
            return Err(SessionError::UnsupportedFormatVersion {
                found: probe.format_version,
            });
        }
        let meta: Self = serde_json::from_str(json)?;
        meta.validate()?;
        Ok(meta)
    }

    /// Validates, then renders `meta.json`.
    ///
    /// # Errors
    /// A [`SessionError`] for a rule violation.
    pub fn to_json(&self) -> Result<String, SessionError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Checks every rule of the format.
    ///
    /// # Errors
    /// [`SessionError::UnsupportedFormatVersion`] or [`SessionError::Invalid`] naming the field.
    pub fn validate(&self) -> Result<(), SessionError> {
        if self.format_version != FORMAT_VERSION {
            return Err(SessionError::UnsupportedFormatVersion {
                found: self.format_version,
            });
        }
        validate_session_id(&self.id)?;
        for (i, chip) in self.chips.iter().enumerate() {
            if self.chips[..i].iter().any(|c| c.name == chip.name) {
                return Err(invalid(format!("chips[{i}].name"), "duplicate chip name"));
            }
            for (field, rate) in [
                ("nominal_rate_hz", chip.nominal_rate_hz),
                ("observed_rate_hz", chip.observed_rate_hz),
            ] {
                if rate.is_some_and(|r| !positive_finite(r)) {
                    return Err(invalid(
                        format!("chips[{i}].{field}"),
                        "must be positive and finite",
                    ));
                }
            }
        }
        for (s, segment) in self.segments.iter().enumerate() {
            if segment.point as usize >= self.probe_points.len() {
                return Err(invalid(
                    format!("segments[{s}].point"),
                    "no such probe point",
                ));
            }
            if self.segments[..s]
                .iter()
                .any(|o| o.axis == segment.axis && o.point == segment.point)
            {
                return Err(invalid(
                    format!("segments[{s}]"),
                    "another segment has the same axis and point",
                ));
            }
            if let Some(w) = segment.window
                && !(w.start.is_finite() && w.end.is_finite() && w.start < w.end)
            {
                return Err(invalid(
                    format!("segments[{s}].window"),
                    "must be finite with start before end",
                ));
            }
            for (f, file) in segment.files.iter().enumerate() {
                let at = format!("segments[{s}].files[{f}]");
                if file.chip as usize >= self.chips.len() {
                    return Err(invalid(format!("{at}.chip"), "no such chip"));
                }
                if segment.files[..f].iter().any(|o| o.chip == file.chip) {
                    return Err(invalid(
                        format!("{at}.chip"),
                        "the chip already has a file in this segment",
                    ));
                }
                if file.sample_file != sample_file_name(file.chip as usize, s) {
                    return Err(invalid(
                        format!("{at}.sample_file"),
                        format!("must be {}", sample_file_name(file.chip as usize, s)),
                    ));
                }
                if file.sample_count as usize > MAX_SAMPLES {
                    return Err(invalid(
                        format!("{at}.sample_count"),
                        "exceeds the sample limit",
                    ));
                }
            }
        }
        Ok(())
    }
}
