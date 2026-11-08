//! Converting a Klipper raw CSV (`OUTPUT=raw_data`) into a session.

use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::fit::Axis;
use wobblescope_core::numeric::median_rate;

use crate::SessionError;
use crate::codec::MAX_SAMPLES;
use crate::meta::{
    ChipMeta, FORMAT_VERSION, Outcome, ProbePoint, SegmentFile, SegmentMeta, SessionMeta, Source,
    TimeDomain, sample_file_name,
};

/// What the caller knows about the file being imported. Klipper's file name does not carry the
/// chip name reliably, so the caller always supplies it.
#[derive(Debug, Clone)]
pub struct ImportRequest {
    /// Id for the new session; see [`crate::meta::validate_session_id`].
    pub id: String,
    /// Creation time as an RFC 3339 UTC string.
    pub created_at: String,
    /// Klipper config section name of the chip that recorded the file.
    pub chip_name: String,
    /// Test axis the file was recorded for.
    pub axis: Axis,
    /// Probe point in machine coordinates, when known.
    pub point: Option<[f64; 3]>,
    /// Name of the imported file, kept in the metadata.
    pub original_file_name: Option<String>,
}

/// A converted file: the metadata and the one recording it describes (`c0-s0.zst`).
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// Validated metadata for a one-chip, one-segment session.
    pub meta: SessionMeta,
    /// The samples, exactly as parsed.
    pub samples: RawSamples,
}

/// Parses a Klipper raw CSV and builds a session around it.
///
/// The chip's health is `Unknown`, the segment covers the whole file with no window (Klipper
/// already cut it), and a note says the probe point is unknown unless `request.point` gives it.
/// A leading BOM, `\r\n` endings, `#` lines and blank lines are accepted; every other line must
/// hold four finite numbers, with timestamps strictly increasing.
///
/// # Errors
/// [`SessionError::CsvRow`] naming the line of a bad row, [`SessionError::CsvTooShort`],
/// [`SessionError::TooManySamples`], or a metadata error such as an invalid id.
pub fn import_klipper_csv(text: &str, request: &ImportRequest) -> Result<Imported, SessionError> {
    let samples = parse_rows(text)?;
    let n = samples.len();
    if n < 2 {
        return Err(SessionError::CsvTooShort { found: n });
    }
    let mut notes = Vec::new();
    if request.point.is_none() {
        notes.push("The probe point is unknown: Klipper's raw CSV does not record it.".to_owned());
    }
    let meta = SessionMeta {
        format_version: FORMAT_VERSION,
        id: request.id.clone(),
        created_at: request.created_at.clone(),
        source: Source::CsvImport {
            original_file_name: request.original_file_name.clone(),
        },
        time_domain: TimeDomain::KlipperPrintTime,
        mode: None,
        settings: None,
        printer: None,
        outcome: Outcome::Complete,
        notes,
        probe_points: vec![ProbePoint {
            position: request.point,
        }],
        chips: vec![ChipMeta {
            name: request.chip_name.clone(),
            chip_type: None,
            nominal_rate_hz: None,
            observed_rate_hz: median_rate(&samples.t),
            axes_map: None,
            mounting: None,
            health: Health::Unknown,
            health_notes: vec!["Imported from a CSV: capture health is unknown.".to_owned()],
        }],
        segments: vec![SegmentMeta {
            axis: request.axis,
            point: 0,
            window: None,
            files: vec![SegmentFile {
                chip: 0,
                sample_file: sample_file_name(0, 0),
                sample_count: n as u32,
                coverage: Coverage::Complete,
                health: None,
                health_notes: Vec::new(),
            }],
        }],
    };
    meta.validate()?;
    Ok(Imported { meta, samples })
}

fn row_error(line: usize, reason: impl Into<String>) -> SessionError {
    SessionError::CsvRow {
        line,
        reason: reason.into(),
    }
}

fn parse_rows(text: &str) -> Result<RawSamples, SessionError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut s = RawSamples::default();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let row = raw.trim();
        if row.is_empty() || row.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = row.split(',').collect();
        if fields.len() != 4 {
            return Err(row_error(
                line,
                format!("expected 4 fields, found {}", fields.len()),
            ));
        }
        let mut v = [0.0f64; 4];
        for (slot, field) in v.iter_mut().zip(&fields) {
            let field = field.trim_ascii();
            if field.is_empty() {
                return Err(row_error(line, "empty field"));
            }
            let x: f64 = field
                .parse()
                .map_err(|_| row_error(line, format!("{field:?} is not a number")))?;
            if !x.is_finite() {
                return Err(row_error(line, format!("{field:?} is not finite")));
            }
            *slot = x;
        }
        if s.t.last().is_some_and(|&prev| v[0] <= prev) {
            return Err(row_error(line, "timestamp does not increase"));
        }
        if s.len() == MAX_SAMPLES {
            return Err(SessionError::TooManySamples {
                claimed: MAX_SAMPLES as u64 + 1,
            });
        }
        s.t.push(v[0]);
        s.x.push(v[1]);
        s.y.push(v[2]);
        s.z.push(v[3]);
    }
    Ok(s)
}

/// What a Klipper raw file name tells about its recording.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FileNameHints {
    /// The test axis, when the name has one of `x` or `y` after `raw_data_`.
    pub axis: Option<Axis>,
    /// The probe point, when the name carries one.
    pub point: Option<[f64; 3]>,
}

/// Reads the axis and, when present, the probe point from a Klipper raw file name
/// (`raw_data_<axis>[_<chip>][_<x>_<y>_<z>]_<NAME>.csv`). Only the base name is read. The chip
/// name is never suggested: chip names and `NAME` may both contain `_`, so it cannot be told
/// apart from the rest of the name.
pub fn suggest_hints_from_file_name(name: &str) -> FileNameHints {
    let base = name.rsplit('/').next().unwrap_or(name);
    let base = base.strip_suffix(".csv").unwrap_or(base);
    let Some(rest) = base.strip_prefix("raw_data_") else {
        return FileNameHints::default();
    };
    let tokens: Vec<&str> = rest.split('_').collect();
    let axis = match tokens.first().copied() {
        Some("x") => Some(Axis::X),
        Some("y") => Some(Axis::Y),
        _ => None,
    };
    if axis.is_none() {
        return FileNameHints::default();
    }
    let point = tokens.windows(3).find_map(|w| {
        Some([
            three_decimals(w[0])?,
            three_decimals(w[1])?,
            three_decimals(w[2])?,
        ])
    });
    FileNameHints { axis, point }
}

/// Parses a token written by Klipper's `%.3f`: optional `-`, digits, `.`, exactly 3 digits.
fn three_decimals(token: &str) -> Option<f64> {
    let digits = token.strip_prefix('-').unwrap_or(token);
    let (int, frac) = digits.split_once('.')?;
    let ok = !int.is_empty()
        && int.bytes().all(|b| b.is_ascii_digit())
        && frac.len() == 3
        && frac.bytes().all(|b| b.is_ascii_digit());
    if ok { token.parse().ok() } else { None }
}
