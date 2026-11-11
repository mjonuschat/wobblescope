//! Welch power spectral density, matching Klipper's `shaper_calibrate`.

use realfft::RealFftPlanner;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::numeric::strictly_increasing;
use crate::samples::RawSamples;

/// Lowest frequency of interest; low-frequency suppression is relative to it.
pub const MIN_FREQ: f64 = 5.0;
/// Default upper analysis frequency.
pub const MAX_FREQ: f64 = 200.0;
/// Welch window length in seconds, rounded up to a power-of-two sample count.
pub const WINDOW_T_SEC: f64 = 0.5;

/// Power spectral densities of one recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CalibrationData {
    /// Source name (for display).
    pub name: String,
    /// Frequency of each bin in Hz.
    pub freq_bins: Vec<f64>,
    /// X-axis PSD.
    pub psd_x: Vec<f64>,
    /// Y-axis PSD.
    pub psd_y: Vec<f64>,
    /// Z-axis PSD.
    pub psd_z: Vec<f64>,
    /// X+Y+Z PSD.
    pub psd_sum: Vec<f64>,
    pub(crate) normalized: bool,
}

/// Modified Bessel function of the first kind, order 0, by its power series.
fn bessel_i0(x: f64) -> f64 {
    let q = x * x / 4.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut k = 1.0;
    while term > sum * 1e-17 {
        term *= q / (k * k);
        sum += term;
        k += 1.0;
    }
    sum
}

/// Kaiser window of length `n` with shape parameter `beta`.
pub(crate) fn kaiser(n: usize, beta: f64) -> Vec<f64> {
    let half = (n as f64 - 1.0) / 2.0;
    let denom = bessel_i0(beta);
    (0..n)
        .map(|i| {
            let r = (i as f64 - half) / half;
            bessel_i0(beta * (1.0 - r * r).max(0.0).sqrt()) / denom
        })
        .collect()
}

/// Welch PSD of one axis; returns one-sided PSD of length `nfft / 2 + 1`.
fn welch(x: &[f64], fs: f64, nfft: usize, window: &[f64]) -> Vec<f64> {
    let scale = 1.0 / window.iter().map(|w| w * w).sum::<f64>();
    let overlap = nfft / 2;
    let step = nfft - overlap;
    let n_windows = (x.len() - overlap) / step;

    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();
    let mut scratch = fft.make_scratch_vec();
    let bins = nfft / 2 + 1;
    let mut acc = vec![0.0; bins];

    for w in 0..n_windows {
        let seg = &x[w * step..w * step + nfft];
        let mean = seg.iter().sum::<f64>() / nfft as f64;
        for (i, v) in input.iter_mut().enumerate() {
            *v = window[i] * (seg[i] - mean);
        }
        fft.process_with_scratch(&mut input, &mut output, &mut scratch)
            .expect("buffers are sized by the planner");
        for (k, c) in output.iter().enumerate() {
            let mut p = c.norm_sqr() * (scale / fs);
            if k != 0 && k != bins - 1 {
                p *= 2.0;
            }
            acc[k] += p;
        }
    }
    acc.iter().map(|v| v / n_windows as f64).collect()
}

impl CalibrationData {
    /// Computes the PSD of a recording as Klipper's `calc_freq_response` does.
    ///
    /// # Errors
    /// Mismatched columns, fewer samples than one window, non-finite or non-increasing
    /// timestamps, and non-finite samples are rejected.
    pub fn from_samples(name: &str, s: &RawSamples) -> Result<Self, CoreError> {
        let n = s.t.len();
        if s.x.len() != n || s.y.len() != n || s.z.len() != n {
            return Err(CoreError::MismatchedColumns);
        }
        if n == 0 {
            return Err(CoreError::TooFewSamples { have: 0, need: 1 });
        }
        if !strictly_increasing(&s.t) || n < 2 {
            return Err(CoreError::InvalidTimestamps);
        }
        if [&s.x, &s.y, &s.z]
            .iter()
            .any(|c| c.iter().any(|v| !v.is_finite()))
        {
            return Err(CoreError::NonFiniteSample);
        }

        let span = s.t[n - 1] - s.t[0];
        let fs = n as f64 / span;
        if !(span.is_finite() && fs.is_finite() && fs > 0.0) {
            return Err(CoreError::InvalidTimestamps);
        }
        let target = fs * WINDOW_T_SEC - 1.0;
        if target < 1.0 {
            return Err(CoreError::InvalidTimestamps);
        }
        if target >= n as f64 {
            return Err(CoreError::TooFewSamples {
                have: n,
                need: n + 1,
            });
        }
        let bit_length = (usize::BITS - (target as usize).leading_zeros()) as usize;
        let m = 1usize << bit_length;
        if n <= m {
            return Err(CoreError::TooFewSamples {
                have: n,
                need: m + 1,
            });
        }

        let window = kaiser(m, 6.0);
        let psd_x = welch(&s.x, fs, m, &window);
        let psd_y = welch(&s.y, fs, m, &window);
        let psd_z = welch(&s.z, fs, m, &window);
        let psd_sum = (0..psd_x.len())
            .map(|i| psd_x[i] + psd_y[i] + psd_z[i])
            .collect();

        let val = 1.0 / (m as f64 * (1.0 / fs));
        let freq_bins = (0..psd_x.len()).map(|k| k as f64 * val).collect();
        Ok(Self {
            name: name.to_owned(),
            freq_bins,
            psd_x,
            psd_y,
            psd_z,
            psd_sum,
            normalized: false,
        })
    }

    /// Whether [`normalize_to_frequencies`](Self::normalize_to_frequencies) has been applied.
    pub fn is_normalized(&self) -> bool {
        self.normalized
    }

    /// Divides each PSD by frequency and suppresses low-frequency noise (Klipper's
    /// `normalize_to_frequencies`). Applying it again has no effect.
    pub fn normalize_to_frequencies(&mut self) {
        if self.normalized {
            return;
        }
        for psd in [
            &mut self.psd_sum,
            &mut self.psd_x,
            &mut self.psd_y,
            &mut self.psd_z,
        ] {
            for (p, &f) in psd.iter_mut().zip(&self.freq_bins) {
                *p /= f + 0.1;
                if f < 2.0 * MIN_FREQ {
                    *p *= (-((2.0 * MIN_FREQ / (f + 0.1)).powi(2)) + 1.0).exp();
                }
            }
        }
        self.normalized = true;
    }
}
