//! The only route from a session to `wobblescope-core`: one [`DatasetInput`] per recording.

use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::DatasetInput;
use wobblescope_core::fit::Axis;

use crate::SessionError;
use crate::meta::{SegmentFile, SessionMeta};

/// Builds the `core` inputs for every recording of `axis`, segments in metadata order and each
/// segment's files in their recorded order.
///
/// Each input carries its chip's name and health, the segment's point position, the file's
/// coverage and the segment window; `include_override` is always `false`, and
/// [`apply_overrides`] sets it for the chips the user chose to include. `load` returns the
/// samples of one file, from a directory or from bytes fetched elsewhere, and its error is
/// returned unchanged.
///
/// # Errors
/// A rule violation in `meta`, the loader's error, or [`SessionError::SampleCountMismatch`]
/// when a loaded recording does not hold the recorded number of samples.
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
            let samples = load(file)?;
            if samples.len() != file.sample_count as usize {
                return Err(SessionError::SampleCountMismatch {
                    file: file.sample_file.clone(),
                    expected: file.sample_count,
                    found: samples.len(),
                });
            }
            let chip = &meta.chips[file.chip as usize];
            inputs.push(DatasetInput {
                chip: chip.name.clone(),
                point: segment.point as usize,
                samples,
                health: chip.health,
                coverage: file.coverage.clone(),
                include_override: false,
                window: segment.window.map(|w| w.start..w.end),
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
