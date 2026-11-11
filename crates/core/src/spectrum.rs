//! Rolling power spectrum of a live accelerometer stream.

use std::collections::VecDeque;

use serde::Serialize;

use crate::error::CoreError;
use crate::psd::{hann, welch};

/// Fewest samples a window may hold.
const MIN_WINDOW_SAMPLES: usize = 16;

/// Power spectral density of the latest window, in input units squared per Hz.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Spectrum {
    /// Frequency of each bin in Hz, up to the configured maximum.
    pub freq_bins: Vec<f64>,
    /// X-axis PSD.
    pub psd_x: Vec<f64>,
    /// Y-axis PSD.
    pub psd_y: Vec<f64>,
    /// Z-axis PSD.
    pub psd_z: Vec<f64>,
    /// X+Y+Z PSD.
    pub psd_sum: Vec<f64>,
}

/// A local maximum of a spectrum.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Peak {
    /// Frequency in Hz.
    pub freq: f64,
    /// PSD value at the peak.
    pub level: f64,
}

/// Welch spectrum over the most recent samples of a three-axis stream.
#[derive(Debug, Clone)]
pub struct LiveSpectrum {
    rate_hz: f64,
    max_hz: f64,
    capacity: usize,
    nfft: usize,
    window: Vec<f64>,
    axes: [VecDeque<f64>; 3],
    next_t: f64,
}

impl LiveSpectrum {
    /// Keeps the latest `ceil(rate_hz * window_seconds)` samples per axis and reports bins up
    /// to `max_hz`. The segment length is the largest power of two within that count.
    ///
    /// # Errors
    /// `InvalidParameter` for non-finite or non-positive arguments, or when fewer than 16
    /// samples would fit in the window.
    pub fn new(rate_hz: f64, window_seconds: f64, max_hz: f64) -> Result<Self, CoreError> {
        let valid = |v: f64| v.is_finite() && v > 0.0;
        if !(valid(rate_hz) && valid(window_seconds) && valid(max_hz)) {
            return Err(CoreError::InvalidParameter(
                "rate, window and range must be positive and finite".into(),
            ));
        }
        let capacity = (rate_hz * window_seconds).ceil();
        if !(capacity >= MIN_WINDOW_SAMPLES as f64 && capacity < 1e9) {
            return Err(CoreError::InvalidParameter(format!(
                "a window of {window_seconds} s at {rate_hz} Hz holds fewer than \
                 {MIN_WINDOW_SAMPLES} samples"
            )));
        }
        let capacity = capacity as usize;
        let nfft = 1usize << capacity.ilog2();
        Ok(Self {
            rate_hz,
            max_hz,
            capacity,
            nfft,
            window: hann(nfft),
            axes: Default::default(),
            next_t: 0.0,
        })
    }

    /// Appends interleaved x, y, z samples; sample `i` is at `t0 + i / rate_hz`. The buffer is
    /// cleared first when `t0` is more than 1.5 sample periods from the expected time.
    ///
    /// # Errors
    /// `MismatchedColumns` when the length is not a multiple of 3, `NonFiniteSample` for a NaN
    /// or infinite value. Nothing changes on error.
    pub fn push_f32(&mut self, t0: f64, xyz: &[f32]) -> Result<(), CoreError> {
        if !xyz.len().is_multiple_of(3) {
            return Err(CoreError::MismatchedColumns);
        }
        if xyz.iter().any(|v| !v.is_finite()) || !t0.is_finite() {
            return Err(CoreError::NonFiniteSample);
        }
        let n = xyz.len() / 3;
        if n == 0 {
            return Ok(());
        }
        if (t0 - self.next_t).abs() > 1.5 / self.rate_hz {
            self.axes.iter_mut().for_each(VecDeque::clear);
        }
        self.next_t = t0 + n as f64 / self.rate_hz;
        for frame in xyz.as_chunks::<3>().0 {
            for (axis, &v) in self.axes.iter_mut().zip(frame) {
                if axis.len() == self.capacity {
                    axis.pop_front();
                }
                axis.push_back(f64::from(v));
            }
        }
        Ok(())
    }

    /// The spectrum of the buffered window, or `None` until it holds one full segment.
    pub fn spectrum(&self) -> Option<Spectrum> {
        if self.axes[0].len() < self.nfft {
            return None;
        }
        let psd = |axis: &VecDeque<f64>| {
            // Segment from the newest sample: welch ends on a whole step, so drop the oldest extra.
            let step = self.nfft / 2;
            let used = self.nfft + (axis.len() - self.nfft) / step * step;
            let x: Vec<f64> = axis.iter().skip(axis.len() - used).copied().collect();
            welch(&x, self.rate_hz, self.nfft, &self.window)
        };
        let width = self.rate_hz / self.nfft as f64;
        let keep = ((self.max_hz / width).floor() as usize + 1).min(self.nfft / 2 + 1);
        let trim = |mut v: Vec<f64>| {
            v.truncate(keep);
            v
        };
        let psd_x = trim(psd(&self.axes[0]));
        let psd_y = trim(psd(&self.axes[1]));
        let psd_z = trim(psd(&self.axes[2]));
        let psd_sum = (0..keep).map(|i| psd_x[i] + psd_y[i] + psd_z[i]).collect();
        Some(Spectrum {
            freq_bins: (0..keep).map(|k| k as f64 * width).collect(),
            psd_x,
            psd_y,
            psd_z,
            psd_sum,
        })
    }
}

/// Local maxima of `psd`, strongest first, no two closer than `min_separation_hz`.
pub fn top_peaks(
    freq_bins: &[f64],
    psd: &[f64],
    count: usize,
    min_separation_hz: f64,
) -> Vec<Peak> {
    let n = freq_bins.len().min(psd.len());
    let mut candidates: Vec<Peak> = (1..n.saturating_sub(1))
        .filter(|&i| psd[i] > psd[i - 1] && psd[i] >= psd[i + 1])
        .map(|i| Peak {
            freq: freq_bins[i],
            level: psd[i],
        })
        .collect();
    candidates.sort_by(|a, b| b.level.total_cmp(&a.level));
    let mut peaks: Vec<Peak> = Vec::new();
    for c in candidates {
        if peaks.len() == count {
            break;
        }
        if peaks
            .iter()
            .all(|p| (p.freq - c.freq).abs() >= min_separation_hz)
        {
            peaks.push(c);
        }
    }
    peaks
}
