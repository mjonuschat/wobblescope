//! The only route from a session to `wobblescope-core`: one [`DatasetInput`] per recording.

use wobblescope_core::RawSamples;
use wobblescope_core::direction::Direction;
use wobblescope_core::eligibility::DatasetInput;

use crate::SessionError;
use crate::meta::{SegmentFile, SessionMeta, undersampled_note};

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
