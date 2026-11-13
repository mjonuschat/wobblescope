//! Session metadata: the `meta.json` shape and the rules a valid session obeys.

use serde::{Deserialize, Serialize};
use wobblescope_core::axes_map::Mounting;
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::fit::Axis;

use crate::SessionError;
use crate::codec::MAX_SAMPLES;

/// Version of the metadata shape this crate writes and reads.
pub const FORMAT_VERSION: u32 = 1;

/// Version of sessions that hold an axes-map calibration capture. Resonance sessions stay at
/// [`FORMAT_VERSION`].
pub const AXES_MAP_FORMAT_VERSION: u32 = 2;

/// Segments of an axes-map session: X, Y, Z.
pub const AXES_MAP_SEGMENTS: usize = 3;

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
        /// Names of every file that was imported, in import order. Sessions written before
        /// multi-file import have none.
        #[serde(default)]
        original_file_names: Vec<String>,
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

/// What a session recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum SessionKind {
    /// A resonance test.
    #[default]
    Resonance,
    /// An axes-map calibration: three segments in the order X, Y, Z.
    AxesMap,
}

/// The moves an axes-map calibration ran with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AxesMapRun {
    /// Speed of the measured moves in mm/s.
    pub speed: f64,
    /// Acceleration of the measured moves in mm/s².
    pub accel: f64,
    /// Z height the moves ran at in mm.
    pub z_height: f64,
    /// Speed of the positioning moves between measured moves in mm/s.
    pub travel_speed: f64,
    /// Z velocity limit applied for a bed-mounted chip in mm/s.
    #[serde(default)]
    pub max_z_velocity: Option<f64>,
    /// Z acceleration limit applied for a bed-mounted chip in mm/s².
    #[serde(default)]
    pub max_z_accel: Option<f64>,
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
    /// The toolhead's square corner velocity in mm/s when the test ran. The smoothing estimate
    /// depends on it.
    #[serde(default)]
    pub square_corner_velocity: Option<f64>,
    #[serde(default)]
    pub damping_ratio_x: Option<f64>,
    #[serde(default)]
    pub damping_ratio_y: Option<f64>,
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
    pub mounting: Option<Mounting>,
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
    /// How this recording's capture looked, judged on this segment alone. `None` when the
    /// capture did not judge it (older sessions, imports); the chip's overall health then stands.
    #[serde(default)]
    pub health: Option<Health>,
    /// What was seen in this segment: counters that rose, gaps in the timestamps.
    #[serde(default)]
    pub health_notes: Vec<String>,
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
    /// What the session recorded. Sessions written before this field hold resonance tests.
    #[serde(default)]
    pub kind: SessionKind,
    /// The moves of an axes-map calibration; present exactly when `kind` is `AxesMap`.
    #[serde(default)]
    pub axes_map_run: Option<AxesMapRun>,
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

/// A chip delivering under this share of its nominal rate is undersampled.
pub const UNDERSAMPLED_SHARE: f64 = 0.5;

/// Why a chip's samples are suspect when its observed rate is under [`UNDERSAMPLED_SHARE`] of
/// its nominal rate, or `None` when it is not, or either rate is unknown. The usual cause is old
/// MCU firmware: before Klipper's 2025-10-06 fix to `sensor_lis2dw.c` a `lis2dw` delivered one
/// sample per 2.5 ms, about 400 Hz of the 1600 Hz it is set to. Samples dropped that way are not
/// filtered first, so vibration above the delivered rate can fold into the analysed band.
pub fn undersampled_note(nominal_hz: Option<f64>, observed_hz: Option<f64>) -> Option<String> {
    let nominal = nominal_hz.filter(|n| positive_finite(*n))?;
    let observed = observed_hz.filter(|o| o.is_finite())?;
    (observed < nominal * UNDERSAMPLED_SHARE).then(|| {
        format!(
            "samples arrive at {observed:.0} Hz, under half the {nominal:.0} Hz Klipper \
             configures; check that the chip's MCU runs current firmware"
        )
    })
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
        if !matches!(
            probe.format_version,
            FORMAT_VERSION | AXES_MAP_FORMAT_VERSION
        ) {
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

    /// The format version this session is written as: [`AXES_MAP_FORMAT_VERSION`] for an
    /// axes-map calibration, [`FORMAT_VERSION`] otherwise.
    pub fn required_version(&self) -> u32 {
        match self.kind {
            SessionKind::AxesMap => AXES_MAP_FORMAT_VERSION,
            SessionKind::Resonance => FORMAT_VERSION,
        }
    }

    /// The recording `chip` made in `segment` (positions in the metadata lists), if any.
    pub fn find_file(&self, chip: u32, segment: usize) -> Option<&SegmentFile> {
        self.segments
            .get(segment)?
            .files
            .iter()
            .find(|f| f.chip == chip)
    }

    /// Checks every rule of the format.
    ///
    /// # Errors
    /// [`SessionError::UnsupportedFormatVersion`] or [`SessionError::Invalid`] naming the field.
    pub fn validate(&self) -> Result<(), SessionError> {
        if self.format_version != self.required_version() {
            return Err(SessionError::UnsupportedFormatVersion {
                found: self.format_version,
            });
        }
        validate_session_id(&self.id)?;
        match self.kind {
            SessionKind::Resonance => {
                if self.axes_map_run.is_some() {
                    return Err(invalid(
                        "axes_map_run",
                        "only an axes-map session has run settings",
                    ));
                }
            }
            SessionKind::AxesMap => {
                if self.axes_map_run.is_none() {
                    return Err(invalid("axes_map_run", "an axes-map session needs it"));
                }
                if self.segments.len() > AXES_MAP_SEGMENTS {
                    return Err(invalid("segments", "an axes-map session has at most three"));
                }
                if let Some(s) = self
                    .segments
                    .iter()
                    .enumerate()
                    .position(|(i, seg)| seg.point as usize != i)
                {
                    return Err(invalid(
                        format!("segments[{s}].point"),
                        "must equal the segment's position",
                    ));
                }
            }
        }
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
