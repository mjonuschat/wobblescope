//! Converting a Klipper raw CSV (`OUTPUT=raw_data`) into a session.

use std::collections::HashMap;

use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::fit::Axis;
use wobblescope_core::numeric::{long_gaps, median_rate, strictly_increasing};
use wobblescope_core::signal::value_problems;

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
/// An import has no live stream, so it is judged on its own timestamps and values: the chip is
/// `Degraded` when the file has a gap longer than three median sample intervals or a
/// sample-value problem (see [`value_problems`]), else `Ok`, and a note names each problem. The
/// parser already refuses repeated or backward timestamps and files over the sample cap. The
/// segment covers the whole file with no window (Klipper already cut it), and a note says the probe point is unknown unless `request.point` gives it.
/// A leading BOM, `\r\n` endings, `#` lines and blank lines are accepted; every other line must
/// hold four finite numbers, with timestamps strictly increasing.
///
/// # Errors
/// [`SessionError::CsvRow`] naming the line of a bad row, [`SessionError::CsvTooShort`],
/// [`SessionError::TooManySamples`], or a metadata error such as an invalid id.
pub fn import_klipper_csv(text: &str, request: &ImportRequest) -> Result<Imported, SessionError> {
    let samples = parse_klipper_csv(text)?;
    let mut group = build_klipper_group(
        &GroupRequest {
            id: request.id.clone(),
            created_at: request.created_at.clone(),
        },
        vec![ImportFile {
            chip_name: request.chip_name.clone(),
            axis: request.axis,
            point: request.point,
            file_name: request.original_file_name.clone(),
            samples,
        }],
    )?;
    let file = group.meta.segments[0].files[0].clone();
    let samples = group.take(&file)?;
    Ok(Imported {
        meta: group.meta,
        samples,
    })
}

#[derive(Debug, Clone)]
pub struct ImportFile {
    pub chip_name: String,
    pub axis: Axis,
    pub point: Option<[f64; 3]>,
    pub file_name: Option<String>,
    pub samples: RawSamples,
}

#[derive(Debug, Clone)]
pub struct GroupRequest {
    /// Id for the new session; see [`crate::meta::validate_session_id`].
    pub id: String,
    pub created_at: String,
}

/// A session built from several files: its metadata and the samples of every file, which
/// [`ImportedGroup::take`] hands out one file at a time.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedGroup {
    pub meta: SessionMeta,
    samples: HashMap<String, RawSamples>,
    sample_count: u64,
}

impl ImportedGroup {
    /// Moves out the samples stored for `file.sample_file`.
    ///
    /// # Errors
    /// [`SessionError::Invalid`] when the group holds no samples under that name, which includes
    /// a file already taken.
    pub fn take(&mut self, file: &SegmentFile) -> Result<RawSamples, SessionError> {
        self.samples
            .remove(&file.sample_file)
            .ok_or_else(|| SessionError::Invalid {
                field: "sample_file".to_owned(),
                reason: format!("no samples held for {}", file.sample_file),
            })
    }

    /// Total number of samples over all files of the group, whether taken or not.
    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }
}

/// Builds one session from parsed files.
///
/// Chips, probe points and segments are listed in order of first appearance. A segment is one
/// (axis, point) pair, and an unknown point equals only another unknown point. Each file keeps
/// the single-file importer's rules: health judged on the file alone, coverage `Complete`, no
/// window. A chip's
/// `observed_rate_hz` is the median rate of its first file.
///
/// # Errors
/// [`SessionError::ImportNoFiles`] for an empty list, [`SessionError::ImportDuplicate`] when a
/// chip appears twice in one segment, [`SessionError::ImportNotSameRun`] when two chips in a
/// segment have time ranges that do not overlap, or a metadata error such as an invalid id.
pub fn build_klipper_group(
    request: &GroupRequest,
    files: Vec<ImportFile>,
) -> Result<ImportedGroup, SessionError> {
    if files.is_empty() {
        return Err(SessionError::ImportNoFiles);
    }
    let names: Vec<String> = files.iter().filter_map(|f| f.file_name.clone()).collect();
    let single_name = if files.len() == 1 {
        names.first().cloned()
    } else {
        None
    };
    let mut notes = Vec::new();
    if files.iter().any(|f| f.point.is_none()) {
        notes.push("The probe point is unknown: Klipper's raw CSV does not record it.".to_owned());
    }

    let mut chips: Vec<ChipMeta> = Vec::new();
    let mut points: Vec<ProbePoint> = Vec::new();
    let mut segments: Vec<SegmentMeta> = Vec::new();
    // Time range of each file per segment, parallel to `segments[..].files`.
    let mut ranges: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut stored = HashMap::new();
    let mut sample_count = 0u64;

    for file in files {
        let chip = match chips.iter().position(|c| c.name == file.chip_name) {
            Some(i) => i,
            None => {
                chips.push(ChipMeta {
                    name: file.chip_name.clone(),
                    chip_type: None,
                    nominal_rate_hz: None,
                    observed_rate_hz: median_rate(&file.samples.t),
                    axes_map: None,
                    mounting: None,
                    health: Health::Ok,
                    health_notes: Vec::new(),
                });
                chips.len() - 1
            }
        };
        let point = match points.iter().position(|p| p.position == file.point) {
            Some(i) => i,
            None => {
                points.push(ProbePoint {
                    position: file.point,
                });
                points.len() - 1
            }
        };
        let segment = match segments
            .iter()
            .position(|s| s.axis == file.axis && s.point == point as u32)
        {
            Some(i) => i,
            None => {
                segments.push(SegmentMeta {
                    axis: file.axis,
                    point: point as u32,
                    window: None,
                    files: Vec::new(),
                });
                ranges.push(Vec::new());
                segments.len() - 1
            }
        };
        if segments[segment]
            .files
            .iter()
            .any(|f| f.chip as usize == chip)
        {
            return Err(SessionError::ImportDuplicate {
                chip: file.chip_name,
                axis: file.axis,
                point: file.point,
            });
        }
        let n = file.samples.len();
        let (Some(&first), Some(&last)) = (file.samples.t.first(), file.samples.t.last()) else {
            return Err(SessionError::CsvTooShort { found: 0 });
        };
        let range = (first, last);
        for (other, other_range) in segments[segment].files.iter().zip(&ranges[segment]) {
            if range.0 > other_range.1 || other_range.0 > range.1 {
                return Err(SessionError::ImportNotSameRun {
                    chip_a: chips[other.chip as usize].name.clone(),
                    chip_b: file.chip_name,
                    axis: file.axis,
                });
            }
        }
        let sample_file = sample_file_name(chip, segment);
        let mut health_notes = long_gaps(&file.samples.t, None);
        health_notes.extend(value_problems(&file.samples));
        let health = Health::judged(!health_notes.is_empty());
        chips[chip].health = chips[chip].health.worse(health);
        chips[chip].health_notes.extend(
            health_notes
                .iter()
                .map(|n| format!("segment {}: {n}", segment + 1)),
        );
        segments[segment].files.push(SegmentFile {
            chip: chip as u32,
            sample_file: sample_file.clone(),
            sample_count: n as u32,
            coverage: Coverage::Complete,
            health: Some(health),
            health_notes,
        });
        ranges[segment].push(range);
        sample_count += n as u64;
        stored.insert(sample_file, file.samples);
    }

    let meta = SessionMeta {
        format_version: FORMAT_VERSION,
        id: request.id.clone(),
        created_at: request.created_at.clone(),
        source: Source::CsvImport {
            original_file_name: single_name,
            original_file_names: names,
        },
        time_domain: TimeDomain::KlipperPrintTime,
        mode: None,
        settings: None,
        printer: None,
        outcome: Outcome::Complete,
        notes,
        probe_points: points,
        chips,
        segments,
    };
    meta.validate()?;
    Ok(ImportedGroup {
        meta,
        samples: stored,
        sample_count,
    })
}

fn row_error(line: usize, reason: impl Into<String>) -> SessionError {
    SessionError::CsvRow {
        line,
        reason: reason.into(),
    }
}

/// Parses a Klipper raw CSV into samples, with each vector sized to the number of data lines.
///
/// Accepts the same text as [`import_klipper_csv`]: a leading BOM, `\r\n` endings, `#` lines and
/// blank lines, and four finite numbers per row with timestamps strictly increasing.
///
/// # Errors
/// [`SessionError::CsvRow`] naming the line of a bad row, [`SessionError::CsvTooShort`] for
/// fewer than two samples, or [`SessionError::TooManySamples`].
pub fn parse_klipper_csv(text: &str) -> Result<RawSamples, SessionError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let count = text
        .lines()
        .filter(|l| {
            let l = l.trim();
            !l.is_empty() && !l.starts_with('#')
        })
        .count();
    let mut s = RawSamples {
        t: Vec::with_capacity(count),
        x: Vec::with_capacity(count),
        y: Vec::with_capacity(count),
        z: Vec::with_capacity(count),
    };
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
        if s.t
            .last()
            .is_some_and(|&prev| !strictly_increasing(&[prev, v[0]]))
        {
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
    if s.len() < 2 {
        return Err(SessionError::CsvTooShort { found: s.len() });
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
