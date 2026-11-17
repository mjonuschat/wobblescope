//! The only route from a session to `wobblescope-core`: one [`DatasetInput`] per recording.

use serde::{Deserialize, Serialize};
use wobblescope_core::RawSamples;
use wobblescope_core::axes_map::{AxesMapInput, AxesMapParams, AxesMapResult, Mounting};
use wobblescope_core::direction::Direction;
use wobblescope_core::eligibility::{
    Coverage, DatasetDecision, DatasetInput, Decision, Health, select_eligible,
};
use wobblescope_core::vibrations::{
    Grid, Kinematics, StepInput, VibrationsParams, VibrationsResult, analyze_chip, worst_case,
};

use crate::SessionError;
use crate::meta::{
    AXES_MAP_DIRECTIONS, Outcome, SegmentFile, SessionKind, SessionMeta, undersampled_note,
};

/// Builds the `core` inputs for every recording of `direction`, segments in metadata order and each
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
    direction: impl Into<Direction>,
    load: F,
) -> Result<Vec<DatasetInput>, SessionError>
where
    F: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    inputs_where(meta, direction.into(), None, load)
}

/// As [`dataset_inputs`], for the recordings of the chip named `chip` only: `load` is never
/// called for another chip's file. A name no chip carries gives no inputs.
///
/// # Errors
/// A rule violation in `meta`.
pub fn dataset_inputs_for_chip<F>(
    meta: &SessionMeta,
    direction: impl Into<Direction>,
    chip: &str,
    load: F,
) -> Result<Vec<DatasetInput>, SessionError>
where
    F: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    inputs_where(meta, direction.into(), Some(chip), load)
}

/// The probe point and the recordings of `chip` for belt A and belt B. The point is the one the
/// session's belt segments share, or 0 when it has none, whatever the chip recorded. Only the chip's files are
/// loaded.
///
/// # Errors
/// A rule violation in `meta`.
pub fn belt_inputs<F>(
    meta: &SessionMeta,
    chip: &str,
    mut load: F,
) -> Result<(usize, Option<DatasetInput>, Option<DatasetInput>), SessionError>
where
    F: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    let a = dataset_inputs_for_chip(meta, Direction::BELT_A, chip, &mut load)?
        .into_iter()
        .next();
    let b = dataset_inputs_for_chip(meta, Direction::BELT_B, chip, &mut load)?
        .into_iter()
        .next();
    let point = meta
        .segments
        .iter()
        .find(|s| s.axis == Direction::BELT_A || s.axis == Direction::BELT_B)
        .map_or(0, |s| s.point as usize);
    Ok((point, a, b))
}

fn inputs_where<F>(
    meta: &SessionMeta,
    direction: Direction,
    only_chip: Option<&str>,
    mut load: F,
) -> Result<Vec<DatasetInput>, SessionError>
where
    F: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    meta.validate()?;
    let mut inputs = Vec::new();
    for segment in meta.segments.iter().filter(|s| s.axis == direction) {
        for file in &segment.files {
            if only_chip.is_some_and(|c| meta.chips[file.chip as usize].name != c) {
                continue;
            }
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

const SEGMENT_LABELS: [&str; AXES_MAP_DIRECTIONS.len()] = ["X", "Y", "Z"];

/// The run settings of an axes-map session as analysis parameters.
///
/// # Errors
/// [`SessionError::Invalid`] when `meta` is not a valid axes-map session.
pub fn axes_map_params(meta: &SessionMeta) -> Result<AxesMapParams, SessionError> {
    meta.validate()?;
    let run = match (meta.kind(), &meta.axes_map_run) {
        (SessionKind::AxesMap, Some(run)) => run,
        _ => {
            return Err(SessionError::Invalid {
                field: "kind".to_owned(),
                reason: "not an axes-map session".to_owned(),
            });
        }
    };
    Ok(AxesMapParams {
        speed: run.max_velocity.map_or(run.speed, |v| run.speed.min(v)),
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
        let mut files = Vec::with_capacity(AXES_MAP_DIRECTIONS.len());
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

        let mut loaded = Vec::with_capacity(AXES_MAP_DIRECTIONS.len());
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
        let Ok(segments) = <[RawSamples; AXES_MAP_DIRECTIONS.len()]>::try_from(loaded) else {
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
                windows: std::array::from_fn(|segment| {
                    meta.segments
                        .get(segment)
                        .and_then(|s| s.window)
                        .map(|w| w.start..w.end)
                }),
                configured_map: chip.axes_map.clone(),
                mounting: if chip.mounting.as_deref() == Some("bed") {
                    Mounting::Bed
                } else {
                    Mounting::Toolhead
                },
            },
            health,
            health_notes,
        });
    }
    Ok(out)
}

/// What a vibrations analysis made of one chip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChipVibrations {
    /// Chip name.
    pub chip: String,
    /// One decision per angle recording the chip made, in angle order.
    pub decisions: Vec<DatasetDecision>,
    /// Why the chip is left out of the worst case; `None` when it takes part.
    pub excluded: Option<String>,
    /// The chip's own profile; `None` when its data could not be analysed or an angle is missing.
    pub result: Option<VibrationsResult>,
    /// Why the chip's data could not be loaded or analysed.
    pub failure: Option<String>,
}

/// The vibrations profile of every chip of a session, and their worst case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VibrationsAnalysis {
    /// One entry per chip, in the order of `meta.chips`.
    pub per_chip: Vec<ChipVibrations>,
    /// The element-wise maximum over the chips that take part; `None` when none does or their
    /// grids cannot be combined.
    pub worst_case: Option<VibrationsResult>,
    /// `wobblescope-core` version that produced it.
    pub core_version: String,
}

fn invalid(field: &str, reason: impl Into<String>) -> SessionError {
    SessionError::Invalid {
        field: field.to_owned(),
        reason: reason.into(),
    }
}

/// The two measured angles and the kinematics of a vibrations session.
fn vibrations_setup(meta: &SessionMeta) -> Result<([f64; 2], Kinematics), SessionError> {
    meta.validate()?;
    let Some(run) = &meta.vibrations_run else {
        return Err(invalid("kind", "not a vibrations session"));
    };
    let (Some(angles), Some(kinematics)) = (run.angles(), Kinematics::parse(&run.kinematics))
    else {
        return Err(invalid(
            "vibrations_run.kinematics",
            format!("unknown kinematics {}", run.kinematics),
        ));
    };
    Ok((angles, kinematics))
}

/// The recording `file` holds, checked against the sample count the metadata records.
fn load_checked<F>(load: &mut F, file: &SegmentFile) -> Result<RawSamples, SessionError>
where
    F: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    let samples = load(file)?;
    if samples.len() == file.sample_count as usize {
        Ok(samples)
    } else {
        Err(SessionError::SampleCountMismatch {
            file: file.sample_file.clone(),
            expected: file.sample_count,
            found: samples.len(),
        })
    }
}

/// The steps of `chip`: each step's window cut from its segment file (`start <= t < end`), the
/// segments in metadata order. Segment 0 is the first angle of the run and segment 1 the second.
/// A segment the chip has no file for gives no steps. Only the chip's files are loaded.
///
/// # Errors
/// `meta` is not a vibrations session or has unknown kinematics, a file cannot be loaded or
/// holds a different number of samples than recorded, or a step window cannot be cut from it.
pub fn vibrations_inputs<F>(
    meta: &SessionMeta,
    chip: &str,
    mut load: F,
) -> Result<Vec<StepInput>, SessionError>
where
    F: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    let (angles, _) = vibrations_setup(meta)?;
    let mut steps = Vec::new();
    for (segment, angle) in meta.segments.iter().zip(angles) {
        let Some(file) = segment
            .files
            .iter()
            .find(|f| meta.chips[f.chip as usize].name == chip)
        else {
            continue;
        };
        let samples = load_checked(&mut load, file)?;
        for step in &segment.steps {
            let range = samples
                .window(step.window.start..step.window.end)
                .map_err(|e| invalid(&file.sample_file, e.to_string()))?;
            steps.push(StepInput {
                angle,
                speed: step.speed,
                samples: samples.slice(range),
            });
        }
    }
    Ok(steps)
}

/// Analyses every chip of a complete vibrations session and the worst case over the chips that
/// take part.
///
/// A chip takes part when each of the two angles has a recording and neither is excluded by
/// health, coverage or an undersampled rate; `include_excluded` lets both take part anyway. A
/// chip left out still gets its own profile when its data can be analysed. A file `load`
/// cannot read, or a profile that cannot be computed, is that chip's `failure` and no other
/// chip's. The worst case is `None` when no chip takes part or their grids differ.
///
/// # Errors
/// `meta` is not a vibrations session, is not complete, or has unknown kinematics.
pub fn analyze_vibrations<F>(
    meta: &SessionMeta,
    max_freq: f64,
    include_excluded: bool,
    mut load: F,
) -> Result<VibrationsAnalysis, SessionError>
where
    F: FnMut(&SegmentFile) -> Result<RawSamples, SessionError>,
{
    let (angles, kinematics) = vibrations_setup(meta)?;
    if meta.outcome != Outcome::Complete {
        return Err(invalid(
            "outcome",
            "the vibrations session is incomplete and cannot be analysed",
        ));
    }
    let params = VibrationsParams {
        max_freq,
        kinematics,
    };

    let mut per_chip = Vec::with_capacity(meta.chips.len());
    let mut grids: Vec<Grid> = Vec::new();
    for chip in &meta.chips {
        // The decisions need only metadata, so the inputs are made without the samples.
        let mut inputs = Vec::new();
        let mut recorded_angles = Vec::new();
        let mut missing_angles = Vec::new();
        for (segment, angle) in angles.iter().enumerate() {
            let recording = meta.segments.get(segment).and_then(|s| {
                dataset_inputs_for_chip(meta, s.axis, &chip.name, |_| Ok(RawSamples::default()))
                    .ok()?
                    .into_iter()
                    .next()
            });
            match recording {
                Some(mut input) => {
                    input.include_override = include_excluded;
                    inputs.push(input);
                    recorded_angles.push(*angle);
                }
                None => missing_angles.push(*angle),
            }
        }
        let (_, eligibility) = select_eligible(&inputs);
        let decisions = eligibility.decisions;

        let mut reasons: Vec<String> = missing_angles
            .iter()
            .map(|angle| format!("no recording at {angle} degrees"))
            .collect();
        for (decision, angle) in decisions.iter().zip(&recorded_angles) {
            if decision.decision == Decision::Excluded {
                reasons.push(format!("{angle} degrees: {}", decision.reasons.join(", ")));
            }
        }
        let excluded = (!reasons.is_empty()).then(|| reasons.join("; "));

        let (mut result, mut failure) = (None, None);
        if missing_angles.is_empty() {
            let analysed = vibrations_inputs(meta, &chip.name, &mut load)
                .map_err(|e| e.to_string())
                .and_then(|steps| analyze_chip(&steps, &params).map_err(|e| e.to_string()));
            match analysed {
                Ok(profile) => result = Some(profile),
                Err(reason) => failure = Some(reason),
            }
        }
        if let (Some(profile), None) = (&result, &excluded) {
            grids.push(Grid {
                angles: profile.angles.clone(),
                speeds: profile.speeds.clone(),
                data: profile.grid.clone(),
            });
        }
        per_chip.push(ChipVibrations {
            chip: chip.name.clone(),
            decisions,
            excluded,
            result,
            failure,
        });
    }
    let grid_refs: Vec<&Grid> = grids.iter().collect();
    Ok(VibrationsAnalysis {
        per_chip,
        worst_case: worst_case(&grid_refs, &params),
        core_version: wobblescope_core::CORE_VERSION.to_owned(),
    })
}
