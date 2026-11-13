//! The only route from a session to `wobblescope-core`: one [`DatasetInput`] per recording.

use serde::Serialize;
use wobblescope_core::RawSamples;
use wobblescope_core::axes_map::{AxesMapInput, AxesMapParams, AxesMapResult, Mounting};
use wobblescope_core::eligibility::{Coverage, DatasetInput, Health};
use wobblescope_core::fit::Axis;

use crate::SessionError;
use crate::meta::{AXES_MAP_SEGMENTS, SegmentFile, SessionKind, SessionMeta, undersampled_note};

/// Builds the `core` inputs for every recording of `axis`, segments in metadata order and each
/// segment's files in their recorded order.
///
/// Each input carries its chip's name, the segment's point position, the file's coverage and
/// the segment window. Its health is the file's own stored health, or the chip's when the file
/// has none. A chip whose observed rate is under half its nominal rate gets that as an
/// exclusion reason on every input. `include_override` is always `false`, and
/// [`apply_overrides`] sets it for the chips the user chose to include. `load` returns the
/// samples of one file, from a directory or from bytes fetched elsewhere; a file it cannot load,
/// or one holding a different number of samples than recorded, becomes an input with no samples
/// and the reason as its load error, which `core` reports as dropped or failed.
///
/// # Errors
/// A rule violation in `meta`.
pub fn dataset_inputs<F>(
    meta: &SessionMeta,
    axis: Axis,
    mut load: F,
) -> Result<Vec<DatasetInput>, SessionError>
where
    F: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    meta.validate()?;
    let mut inputs = Vec::new();
    for segment in meta.segments.iter().filter(|s| s.axis == axis) {
        for file in &segment.files {
            let chip = &meta.chips[file.chip as usize];
            let loaded = load(file).and_then(|samples| {
                if samples.len() == file.sample_count as usize {
                    Ok(samples)
                } else {
                    Err(SessionError::SampleCountMismatch {
                        file: file.sample_file.clone(),
                        expected: file.sample_count,
                        found: samples.len(),
                    })
                }
            });
            let (samples, load_error) = match loaded {
                Ok(samples) => (samples, None),
                Err(e) => (RawSamples::default(), Some(e.to_string())),
            };
            inputs.push(DatasetInput {
                chip: chip.name.clone(),
                point: segment.point as usize,
                samples,
                health: file.health.unwrap_or(chip.health),
                coverage: file.coverage.clone(),
                include_override: false,
                window: segment.window.map(|w| w.start..w.end),
                exclusions: undersampled_note(chip.nominal_rate_hz, chip.observed_rate_hz)
                    .into_iter()
                    .collect(),
                load_error,
            });
        }
    }
    Ok(inputs)
}

/// Sets `include_override` on every input of the chips named in `chips`, which makes `core`
/// use a dataset that health or coverage would exclude and report it as overridden. A name no
/// input carries is ignored.
pub fn apply_overrides(inputs: &mut [DatasetInput], chips: &[String]) {
    for input in inputs {
        if chips.contains(&input.chip) {
            input.include_override = true;
        }
    }
}

/// What an axes-map analysis made of one chip.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "state")]
#[allow(
    clippy::large_enum_variant,
    reason = "short-lived per-chip values, one per accelerometer"
)]
pub enum AxesMapChip {
    /// The chip had all three recordings and they were analysed.
    Analysed {
        /// Chip name.
        chip: String,
        /// Worst capture health of the chip and its three recordings.
        health: Health,
        /// Notes behind the health value.
        health_notes: Vec<String>,
        /// The analysis.
        result: AxesMapResult,
    },
    /// The chip lacks a usable recording.
    Incomplete {
        /// Chip name.
        chip: String,
        /// The first missing piece.
        missing: String,
    },
    /// The analysis of the chip failed.
    Failed {
        /// Chip name.
        chip: String,
        /// Why.
        message: String,
    },
}

/// One chip of an axes-map session, ready for analysis or not.
#[allow(
    clippy::large_enum_variant,
    reason = "short-lived per-chip values, one per accelerometer"
)]
pub enum ChipCapture {
    /// All three recordings are present, complete and readable.
    Ready {
        /// The input for `analyze_axes_map`.
        input: AxesMapInput,
        /// Worst of the chip's health and its three recordings' health.
        health: Health,
        /// The chip's notes and the recordings' notes, without repeats.
        health_notes: Vec<String>,
    },
    /// The chip lacks a usable recording.
    Incomplete {
        /// Chip name.
        chip: String,
        /// The first missing piece.
        missing: String,
    },
}

const SEGMENT_LABELS: [&str; AXES_MAP_SEGMENTS] = ["X", "Y", "Z"];

/// The run settings of an axes-map session as analysis parameters.
///
/// # Errors
/// [`SessionError::Invalid`] when `meta` is not a valid axes-map session.
pub fn axes_map_params(meta: &SessionMeta) -> Result<AxesMapParams, SessionError> {
    meta.validate()?;
    let run = match (&meta.kind, &meta.axes_map_run) {
        (SessionKind::AxesMap, Some(run)) => run,
        _ => {
            return Err(SessionError::Invalid {
                field: "kind".to_owned(),
                reason: "not an axes-map session".to_owned(),
            });
        }
    };
    Ok(AxesMapParams {
        speed: run.speed,
        accel: run.accel,
        max_z_velocity: run.max_z_velocity,
        max_z_accel: run.max_z_accel,
    })
}

/// Builds one [`ChipCapture`] per chip of an axes-map session, in the order of `meta.chips`.
///
/// A chip is `Ready` only when each of the three segments (X, Y, Z) has a file of that chip
/// with complete coverage. Otherwise it is `Incomplete`, naming the first missing piece: a
/// segment without a recording, the coverage's own description, or a recording `load` cannot
/// read or that holds a different number of samples than recorded. A bad recording affects its
/// chip only.
///
/// # Errors
/// A rule violation in `meta`, or `meta` is not an axes-map session.
pub fn axes_map_inputs<F>(meta: &SessionMeta, mut load: F) -> Result<Vec<ChipCapture>, SessionError>
where
    F: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    axes_map_params(meta)?;
    let mut out = Vec::with_capacity(meta.chips.len());
    for (index, chip) in meta.chips.iter().enumerate() {
        let incomplete = |missing: String| ChipCapture::Incomplete {
            chip: chip.name.clone(),
            missing,
        };
        let mut files = Vec::with_capacity(AXES_MAP_SEGMENTS);
        let mut problem = None;
        for (segment, label) in SEGMENT_LABELS.iter().enumerate() {
            match meta.find_file(index as u32, segment) {
                None => {
                    problem = Some(format!("segment {label} has no recording"));
                    break;
                }
                Some(file) => match &file.coverage {
                    Coverage::Complete => files.push(file),
                    Coverage::Incomplete { missing } => {
                        problem = Some(missing.clone());
                        break;
                    }
                },
            }
        }
        if let Some(missing) = problem {
            out.push(incomplete(missing));
            continue;
        }

        let mut loaded = Vec::with_capacity(AXES_MAP_SEGMENTS);
        for (file, label) in files.iter().zip(SEGMENT_LABELS) {
            let result = load(file).and_then(|samples| {
                if samples.len() == file.sample_count as usize {
                    Ok(samples)
                } else {
                    Err(SessionError::SampleCountMismatch {
                        file: file.sample_file.clone(),
                        expected: file.sample_count,
                        found: samples.len(),
                    })
                }
            });
            match result {
                Ok(samples) => loaded.push(samples),
                Err(e) => {
                    problem = Some(format!("segment {label}: {e}"));
                    break;
                }
            }
        }
        if let Some(missing) = problem {
            out.push(incomplete(missing));
            continue;
        }
        let Ok(segments) = <[RawSamples; AXES_MAP_SEGMENTS]>::try_from(loaded) else {
            unreachable!("three files were loaded");
        };

        let mut health = chip.health;
        let mut health_notes: Vec<String> = Vec::new();
        let notes = chip
            .health_notes
            .iter()
            .chain(files.iter().flat_map(|f| &f.health_notes));
        for note in notes {
            if !health_notes.contains(note) {
                health_notes.push(note.clone());
            }
        }
        for file in &files {
            if file.health == Some(Health::Degraded) {
                health = Health::Degraded;
            }
        }
        out.push(ChipCapture::Ready {
            input: AxesMapInput {
                chip: chip.name.clone(),
                segments,
                configured_map: chip.axes_map.clone(),
                mounting: chip.mounting.unwrap_or(Mounting::Toolhead),
            },
            health,
            health_notes,
        });
    }
    Ok(out)
}
