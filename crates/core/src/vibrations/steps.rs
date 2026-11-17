//! Per-step PSDs and energies: the first loop of `VibrationsComputation.compute`.

use crate::error::CoreError;
use crate::numeric::{interp, trapz};
use crate::psd::CalibrationData;
use crate::samples::RawSamples;

/// One constant-speed pass of the test, recorded at a movement angle.
#[derive(Debug, Clone, PartialEq)]
pub struct StepInput {
    /// Movement angle in degrees.
    pub angle: f64,
    /// Movement speed in mm/s.
    pub speed: f64,
    /// The accelerometer recording of the pass.
    pub samples: RawSamples,
}

/// The spectrum of one step on the shared frequency grid.
#[derive(Debug, Clone, PartialEq)]
pub struct StepPsd {
    /// Movement angle in degrees.
    pub angle: f64,
    /// Movement speed in mm/s.
    pub speed: f64,
    /// The summed X, Y and Z PSD interpolated on the target frequencies.
    pub psd: Vec<f64>,
    /// Trapezoid integral of the summed PSD up to the maximum frequency, over the step's own bins.
    pub energy: f64,
}

/// The target frequencies and the spectrum of every step.
///
/// The target frequencies are the bins of the first step up to `max_freq`. A later step with the
/// same angle and speed replaces the earlier one in place, as the dictionary Shake&Tune keeps does.
///
/// # Errors
/// [`CoreError::InvalidParameter`] when there are no steps, and whatever
/// [`CalibrationData::from_samples`] rejects in a recording.
pub fn step_psds(
    steps: &[StepInput],
    max_freq: f64,
) -> Result<(Vec<f64>, Vec<StepPsd>), CoreError> {
    if steps.is_empty() {
        return Err(CoreError::InvalidParameter(
            "the vibrations profile needs at least one step".into(),
        ));
    }
    let mut target_freqs: Vec<f64> = Vec::new();
    let mut out: Vec<StepPsd> = Vec::with_capacity(steps.len());
    for (index, step) in steps.iter().enumerate() {
        let data = CalibrationData::from_samples("", &step.samples)?;
        let keep = data
            .freq_bins
            .iter()
            .take_while(|f| **f <= max_freq)
            .count();
        let freqs = &data.freq_bins[..keep];
        let psd_sum = &data.psd_sum[..keep];
        if index == 0 {
            target_freqs = freqs.to_vec();
        }
        let entry = StepPsd {
            angle: step.angle,
            speed: step.speed,
            psd: interp(&target_freqs, freqs, psd_sum),
            energy: trapz(psd_sum, freqs),
        };
        match out
            .iter_mut()
            .find(|e| e.angle == step.angle && e.speed == step.speed)
        {
            Some(existing) => *existing = entry,
            None => out.push(entry),
        }
    }
    Ok((target_freqs, out))
}
