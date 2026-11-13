//! Peaks of a PSD, found the way Shake&Tune finds them (`detect_peaks` and the thresholds of its
//! shaper graph, at the commit pinned in `docs/specs/shaketune-deviations.md`).
//!
//! The detector is kept exactly as Shake&Tune writes it, including its quirks. Peaks closer
//! than the smoothing window merge, the last two bins are never examined, two peaks can be
//! refined to the same bin, and a peak refined to a negative index wraps to the end of the
//! array, as Python indexes. Each deliberate difference is a row in the deviations file.

use serde::{Deserialize, Serialize};

use crate::psd::CalibrationData;

/// A peak must rise above this fraction of the strongest value to be marked.
pub const PEAK_DETECTION: f64 = 0.05;
/// A peak above this fraction of the strongest value is flagged as one that matters.
pub const PEAK_EFFECT: f64 = 0.12;
/// Width of the moving average the curve is smoothed with before peaks are looked for.
const WINDOW: usize = 5;
/// A peak is moved to the largest raw value within this many bins.
const VICINITY: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PsdPeak {
    /// Frequency in Hz.
    pub freq: f64,
    pub level: f64,
    pub above_effect: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PsdPeaks {
    /// The peaks, in the order found (by frequency).
    pub peaks: Vec<PsdPeak>,
    /// The lower threshold line, `PEAK_DETECTION` times the strongest value.
    pub detect_threshold: f64,
    /// The upper threshold line, `PEAK_EFFECT` times the strongest value.
    pub effect_threshold: f64,
}

/// Indices into `psd` of Shake&Tune's `detect_peaks` (window 5, vicinity 3).
pub fn detect_peaks(psd: &[f64], threshold: f64) -> Vec<usize> {
    detect_peaks_with(psd, threshold, WINDOW, VICINITY)
}

/// Indices into `psd` of the local maxima of the curve smoothed with a moving average of `window`
/// bins above `threshold`, each moved to the largest raw value within `vicinity` bins.
/// Fewer than `window` bins have no peaks.
pub fn detect_peaks_with(
    psd: &[f64],
    threshold: f64,
    window: usize,
    vicinity: usize,
) -> Vec<usize> {
    let n = psd.len();
    if n < window {
        return Vec::new();
    }
    let kernel = 1.0 / window as f64;
    let pad = psd[..window].iter().sum::<f64>() / window as f64;
    let mut smoothed = vec![pad; window / 2];
    // numpy accumulates the convolution with fused multiply-add. The padded mean and the first
    // window are equal except for the last bit, and the strict comparisons below see that bit.
    smoothed.extend(
        psd.windows(window)
            .map(|w| w.iter().fold(0.0, |acc, v| v.mul_add(kernel, acc))),
    );
    let mut peaks = Vec::new();
    for j in 0..smoothed.len() - 2 {
        let p = j + 1;
        if smoothed[j] < smoothed[p] && smoothed[p] > smoothed[j + 2] && smoothed[p] > threshold {
            let start = p.saturating_sub(vicinity);
            let end = (p + vicinity + 1).min(n);
            let offset = psd[start..end].iter().enumerate().fold(0, |best, (k, v)| {
                if *v > psd[start + best] { k } else { best }
            });
            let index = p as isize + offset as isize - vicinity as isize;
            peaks.push(if index < 0 {
                (index + n as isize) as usize
            } else {
                index as usize
            });
        }
    }
    peaks
}

/// The peaks of `data.psd_sum` up to `max_freq`, with Shake&Tune's thresholds at 5% and 12% of
/// its strongest value. `None` when no bin up to `max_freq` has power.
pub fn psd_peaks(data: &CalibrationData, max_freq: f64) -> Option<PsdPeaks> {
    let kept = data
        .freq_bins
        .iter()
        .take_while(|f| **f <= max_freq)
        .count();
    let psd = &data.psd_sum[..kept.min(data.psd_sum.len())];
    let max = psd.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if max.is_nan() || max <= 0.0 {
        return None;
    }
    let detect_threshold = PEAK_DETECTION * max;
    let effect_threshold = PEAK_EFFECT * max;
    let peaks = detect_peaks(psd, detect_threshold)
        .into_iter()
        .map(|i| PsdPeak {
            freq: data.freq_bins[i],
            level: psd[i],
            above_effect: psd[i] > effect_threshold,
        })
        .collect();
    Some(PsdPeaks {
        peaks,
        detect_threshold,
        effect_threshold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(psd_sum: Vec<f64>) -> CalibrationData {
        CalibrationData {
            name: "t".to_owned(),
            freq_bins: (0..psd_sum.len()).map(|i| i as f64).collect(),
            psd_x: psd_sum.clone(),
            psd_y: vec![0.0; psd_sum.len()],
            psd_z: vec![0.0; psd_sum.len()],
            psd_sum,
            normalized: true,
        }
    }

    #[test]
    fn a_psd_without_power_has_no_peaks_report() {
        assert_eq!(psd_peaks(&data(vec![0.0; 20]), 200.0), None);
        assert_eq!(psd_peaks(&data(vec![]), 200.0), None);
    }

    #[test]
    fn only_bins_up_to_max_freq_count() {
        let mut psd = vec![0.0; 40];
        for (i, v) in [0.2, 0.6, 1.0, 0.6, 0.2].into_iter().enumerate() {
            psd[8 + i] = v;
            psd[28 + i] = 100.0 * v;
        }
        let got = psd_peaks(&data(psd), 20.0).unwrap();
        assert_eq!(got.peaks.len(), 1);
        assert_eq!(got.peaks[0].freq, 10.0);
        assert_eq!(got.detect_threshold, 0.05);
        assert!(got.peaks[0].above_effect);
    }
}
