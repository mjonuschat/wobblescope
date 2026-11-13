//! The time-frequency map Shake&Tune draws under its shaper graph (`compute_spectrogram`, at the
//! commit pinned in `docs/specs/shaketune-deviations.md`).
//!
//! Each segment of the recording gets the same window length as the PSD and 50% overlap. Its
//! mean is removed, a Kaiser window applied, and the power spectra of the three axes are added.
//! The result is the raw PSD of each segment: not divided by frequency, and with no low-frequency
//! suppression, unlike the normalised PSD of the shaper graph.

use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};

use crate::eligibility::DatasetInput;
use crate::error::CoreError;
use crate::numeric::strictly_increasing;
use crate::psd::{WINDOW_T_SEC, kaiser};
use crate::samples::RawSamples;

/// Percent of the matrix below the colour floor.
const FLOOR_PERCENT: f64 = 5.0;

/// The power of one recording over time and frequency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Spectrogram {
    /// Frequency of each column in Hz, from 0 up to the requested maximum.
    pub freq_bins: Vec<f64>,
    /// Time of each row in seconds from the start of the recording.
    pub times: Vec<f64>,
    /// One row per entry of `times`, one value per entry of `freq_bins`.
    pub power: Vec<Vec<f64>>,
    /// The lower end of the colour scale, from the whole matrix including the frequencies not
    /// returned.
    pub floor: f64,
    /// The largest value of the whole matrix.
    pub max: f64,
}

/// `percent` of `values` the way numpy computes it, interpolating linearly between ranks. `None`
/// for no values.
pub fn percentile(values: &[f64], percent: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = percent / 100.0 * (sorted.len() - 1) as f64;
    let (lower, upper) = (rank.floor() as usize, rank.ceil() as usize);
    let fraction = rank - lower as f64;
    Some(sorted[lower] + (sorted[upper] - sorted[lower]) * fraction)
}

/// Shake&Tune's spectrogram of `samples`, keeping the frequencies up to `max_freq`.
///
/// # Errors
/// The checks of [`crate::CalibrationData::from_samples`] (mismatched columns, bad timestamps,
/// non-finite samples), [`CoreError::TooFewSamples`] for a recording shorter than one segment and
/// [`CoreError::FlatSignal`] for one with no power at all.
pub fn spectrogram(samples: &RawSamples, max_freq: f64) -> Result<Spectrogram, CoreError> {
    let n = samples.t.len();
    if [&samples.x, &samples.y, &samples.z]
        .iter()
        .any(|c| c.len() != n)
    {
        return Err(CoreError::MismatchedColumns);
    }
    if n == 0 {
        return Err(CoreError::TooFewSamples { have: 0, need: 1 });
    }
    if !strictly_increasing(&samples.t) || n < 2 {
        return Err(CoreError::InvalidTimestamps);
    }
    if [&samples.x, &samples.y, &samples.z]
        .iter()
        .any(|c| c.iter().any(|v| !v.is_finite()))
    {
        return Err(CoreError::NonFiniteSample);
    }
    let span = samples.t[n - 1] - samples.t[0];
    let fs = n as f64 / span;
    let target = fs * WINDOW_T_SEC - 1.0;
    if !(span.is_finite() && fs.is_finite() && fs > 0.0) || target < 1.0 {
        return Err(CoreError::InvalidTimestamps);
    }
    let bit_length = (usize::BITS - (target as usize).leading_zeros()) as usize;
    let nperseg = 1usize << bit_length;
    if n < nperseg {
        return Err(CoreError::TooFewSamples {
            have: n,
            need: nperseg,
        });
    }
    let step = nperseg - nperseg / 2;
    let segments = 1 + (n - nperseg) / step;
    let bins = nperseg / 2 + 1;
    let window = kaiser(nperseg, 6.0);
    let scale = 1.0 / (fs * window.iter().map(|w| w * w).sum::<f64>());

    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(nperseg);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();
    let mut scratch = fft.make_scratch_vec();

    let mut matrix: Vec<Vec<f64>> = Vec::with_capacity(segments);
    for i in 0..segments {
        let mut row = vec![0.0; bins];
        for axis in [&samples.x, &samples.y, &samples.z] {
            let segment = &axis[i * step..i * step + nperseg];
            let mean = segment.iter().sum::<f64>() / nperseg as f64;
            for (k, v) in input.iter_mut().enumerate() {
                *v = window[k] * (segment[k] - mean);
            }
            fft.process_with_scratch(&mut input, &mut output, &mut scratch)
                .expect("buffers are sized by the planner");
            for (k, c) in output.iter().enumerate() {
                let mut power = c.norm_sqr() * scale;
                if k != 0 && k != bins - 1 {
                    power *= 2.0;
                }
                row[k] += power;
            }
        }
        matrix.push(row);
    }

    let all: Vec<f64> = matrix.iter().flatten().copied().collect();
    let max = all.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if max.is_nan() || max <= 0.0 {
        return Err(CoreError::FlatSignal);
    }
    let mut floor = percentile(&all, FLOOR_PERCENT).expect("a matrix has values");
    if floor <= 0.0 {
        // D7: Shake&Tune's log colour scale fails for a floor of zero, so it has none to match.
        floor = all
            .iter()
            .copied()
            .filter(|v| *v > 0.0)
            .fold(f64::INFINITY, f64::min);
    }
    let width = 1.0 / (nperseg as f64 * (1.0 / fs));
    let freq_bins: Vec<f64> = (0..bins)
        .map(|k| k as f64 * width)
        .take_while(|f| *f <= max_freq)
        .collect();
    let times = (0..segments)
        .map(|i| (i * step) as f64 / fs + nperseg as f64 / (2.0 * fs))
        .collect();
    let power = matrix
        .into_iter()
        .map(|mut row| {
            row.truncate(freq_bins.len());
            row
        })
        .collect();
    Ok(Spectrogram {
        freq_bins,
        times,
        power,
        floor,
        max,
    })
}

/// The spectrogram of one dataset: its samples cut to its time window, as `analyze` cuts them.
///
/// # Errors
/// The input's own load error as [`CoreError::InvalidParameter`], a window with no samples, and
/// the errors of [`spectrogram`].
pub fn spectrogram_of(input: &DatasetInput, max_freq: f64) -> Result<Spectrogram, CoreError> {
    if let Some(message) = &input.load_error {
        return Err(CoreError::InvalidParameter(message.clone()));
    }
    match &input.window {
        Some(window) => {
            let kept = input.samples.window(window.clone())?;
            spectrogram(&input.samples.slice(kept), max_freq)
        }
        None => spectrogram(&input.samples, max_freq),
    }
}
