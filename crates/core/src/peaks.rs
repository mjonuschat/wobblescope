//! Peaks of a PSD, found the way Shake&Tune finds them (`detect_peaks` and the thresholds of its
//! shaper graph, at the commit pinned in `docs/specs/shaketune-deviations.md`).
//!
//! The detector is kept as Shake&Tune writes it, including its quirks. The one difference on the
//! shaper graph is a narrower smoothing window ([`PSD_WINDOW`]), so close peaks stay apart. Peaks closer
//! than the smoothing window merge and the last two bins are never examined. Deliberate
//! differences: a peak refined to a negative index is kept near the start instead of wrapping
//! to the end of the array, and a bin that two peaks refine to is reported once.

use serde::{Deserialize, Serialize};

use crate::psd::CalibrationData;

/// A peak must rise above this fraction of the strongest value to be marked.
pub const PEAK_DETECTION: f64 = 0.05;
/// A peak above this fraction of the strongest value is flagged as one that matters.
pub const PEAK_EFFECT: f64 = 0.12;
/// Width of the moving average the curve is smoothed with before peaks are looked for.
const WINDOW: usize = 5;
/// The smoothing window of [`psd_peaks`]. Shake&Tune's 5 bins span 8 Hz on a 1.56 Hz grid, which
/// joins peaks that close and lowers a narrow one to about 80% of its height.
pub const PSD_WINDOW: usize = 3;
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
    detect_peaks_filtered(psd, threshold, None, window, vicinity)
}

/// [`detect_peaks_with`] with Shake&Tune's `relative_height_threshold`: a peak of the smoothed
/// curve is kept only when it rises above the lowest smoothed value within `vicinity` bins by more
/// than `relative_height` times its own height.
pub fn detect_peaks_relative(
    data: &[f64],
    threshold: f64,
    relative_height: f64,
    window: usize,
    vicinity: usize,
) -> Vec<usize> {
    detect_peaks_filtered(data, threshold, Some(relative_height), window, vicinity)
}

fn detect_peaks_filtered(
    psd: &[f64],
    threshold: f64,
    relative_height: Option<f64>,
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
    peaks_of_smoothed(
        psd,
        &smoothed,
        threshold,
        relative_height,
        None,
        vicinity,
        false,
    )
}

/// [`detect_peaks_with`] for a curve whose whole length matters, as the belt comparison needs.
///
/// Shake&Tune's moving average starts `window / 2` bins in, never reaches the last half
/// window, and for an even window sits half a bin off the bin it is assigned to, so a peak in the
/// first or last bins is missed and a symmetric peak on a bin ties with its neighbour and is
/// missed too. Here the average is centred on each bin (an even window gives its two end bins half
/// weight) and the curve is padded with its end values, so every bin has a smoothed value. A peak
/// whose top is two or more equal smoothed values is reported at the first of them: a symmetric
/// peak between two bins gives exactly that, and a strict comparison would find no peak.
pub fn detect_peaks_whole(
    psd: &[f64],
    threshold: f64,
    window: usize,
    vicinity: usize,
) -> Vec<usize> {
    if psd.len() < 3 {
        return Vec::new();
    }
    let smoothed = smooth_centred(psd, window.max(1));
    peaks_of_smoothed(psd, &smoothed, threshold, None, None, vicinity, true)
}

/// [`detect_peaks_whole`] that also drops a peak whose prominence is at most `min_prominence`
/// times its own smoothed height. The prominence is the height above the higher of the
/// two lowest points reached on the way to a higher point of the smoothed curve, or to its end.
/// A ripple on a plateau between two real peaks has a prominence near 0 and is dropped. A
/// resonance stands well clear of it.
pub fn detect_peaks_whole_prominent(
    psd: &[f64],
    threshold: f64,
    window: usize,
    vicinity: usize,
    min_prominence: f64,
) -> Vec<usize> {
    if psd.len() < 3 {
        return Vec::new();
    }
    let smoothed = smooth_centred(psd, window.max(1));
    peaks_of_smoothed(
        psd,
        &smoothed,
        threshold,
        None,
        Some(min_prominence),
        vicinity,
        true,
    )
}

/// Height of `smoothed[p]` above the higher of the lowest values reached on each side before a
/// higher value, or the end of the curve, is met.
fn prominence(smoothed: &[f64], p: usize) -> f64 {
    let top = smoothed[p];
    let side = |values: &mut dyn Iterator<Item = f64>| {
        let mut lowest = top;
        for v in values {
            if v > top {
                break;
            }
            lowest = lowest.min(v);
        }
        lowest
    };
    let left = side(&mut smoothed[..p].iter().rev().copied());
    let right = side(&mut smoothed[p + 1..].iter().copied());
    top - left.max(right)
}

fn smooth_centred(psd: &[f64], window: usize) -> Vec<f64> {
    let half = window / 2;
    let last = psd.len() - 1;
    let end_weight = if window.is_multiple_of(2) { 0.5 } else { 1.0 };
    (0..psd.len())
        .map(|i| {
            let sum: f64 = (0..=2 * half)
                .map(|k| {
                    let at = (i + k).saturating_sub(half).min(last);
                    let weight = if k == 0 || k == 2 * half {
                        end_weight
                    } else {
                        1.0
                    };
                    weight * psd[at]
                })
                .sum();
            sum / window as f64
        })
        .collect()
}

/// Local maxima of `smoothed` above `threshold`, each moved to the largest raw value within
/// `vicinity` bins, counted from the start of that window and listed once.
fn peaks_of_smoothed(
    psd: &[f64],
    smoothed: &[f64],
    threshold: f64,
    relative_height: Option<f64>,
    min_prominence: Option<f64>,
    vicinity: usize,
    first_of_plateau: bool,
) -> Vec<usize> {
    let n = psd.len();
    let mut peaks = Vec::new();
    for j in 0..smoothed.len().saturating_sub(2) {
        let p = j + 1;
        let falls = if first_of_plateau {
            smoothed[p] >= smoothed[j + 2]
        } else {
            smoothed[p] > smoothed[j + 2]
        };
        if smoothed[j] < smoothed[p] && falls && smoothed[p] > threshold {
            if min_prominence.is_some_and(|m| prominence(smoothed, p) <= m * smoothed[p]) {
                continue;
            }
            if let Some(relative) = relative_height {
                let around =
                    &smoothed[p.saturating_sub(vicinity)..(p + vicinity + 1).min(smoothed.len())];
                let lowest = around.iter().copied().fold(f64::INFINITY, f64::min);
                if smoothed[p] - lowest <= relative * smoothed[p] {
                    continue;
                }
            }
            let start = p.saturating_sub(vicinity);
            let end = (p + vicinity + 1).min(n);
            let offset = psd[start..end].iter().enumerate().fold(0, |best, (k, v)| {
                if *v > psd[start + best] { k } else { best }
            });
            // Shake&Tune computes `p + offset - vicinity`, which is negative for a peak
            // within `vicinity` bins of the start and wraps to the end of the array.
            let index = start + offset;
            // Shake&Tune reports a bin once for every smoothed peak that refines to it.
            if !peaks.contains(&index) {
                peaks.push(index);
            }
        }
    }
    peaks
}

/// The peaks of `data.psd_sum` up to `max_freq`, with Shake&Tune's thresholds at 5% and 12% of
/// its strongest value. `None` when no bin up to `max_freq` has power.
pub fn psd_peaks(data: &CalibrationData, max_freq: f64) -> Option<PsdPeaks> {
    psd_peaks_with(data, max_freq, PSD_WINDOW)
}

/// [`psd_peaks`] with a smoothing `window` of the caller's choosing. Shake&Tune's is 5.
pub fn psd_peaks_with(data: &CalibrationData, max_freq: f64, window: usize) -> Option<PsdPeaks> {
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
    let peaks = detect_peaks_with(psd, detect_threshold, window, VICINITY)
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

    #[test]
    fn relative_height_drops_a_peak_that_barely_rises() {
        let mut psd = vec![10.0; 30];
        psd[10..13].copy_from_slice(&[10.2, 10.4, 10.2]);
        psd[20..23].copy_from_slice(&[14.0, 20.0, 14.0]);
        assert_eq!(detect_peaks_with(&psd, 1.0, 3, 3).len(), 2);
        assert_eq!(detect_peaks_relative(&psd, 1.0, 0.04, 3, 3), vec![21]);
    }

    /// A smoothed peak at bin 2 whose largest raw value is also at bin 2, with `vicinity` 5.
    fn peak_near_the_start() -> Vec<f64> {
        let mut psd = vec![0.0; 30];
        psd[1..4].copy_from_slice(&[0.1, 1.0, 0.3]);
        psd
    }

    #[test]
    fn a_peak_near_the_start_is_not_wrapped_to_the_end() {
        let got = detect_peaks_with(&peak_near_the_start(), 0.05, 3, 5);
        assert_eq!(got, vec![2]);
    }

    #[test]
    fn a_peak_near_the_start_is_found_at_its_largest_raw_bin() {
        let mut psd = peak_near_the_start();
        psd[4] = 1.5;
        let got = detect_peaks_with(&psd, 0.05, 3, 5);
        assert!(got.contains(&4), "{got:?}");
        assert!(got.iter().all(|i| *i < psd.len() - 3), "{got:?}");
    }

    #[test]
    fn two_peaks_that_refine_to_the_same_bin_are_reported_once() {
        let mut psd = vec![0.0; 30];
        psd[9..12].copy_from_slice(&[0.2, 1.0, 0.2]);
        psd[13..16].copy_from_slice(&[0.2, 0.9, 0.2]);
        assert_eq!(detect_peaks_with(&psd, 0.05, 3, 5), vec![10]);
    }

    #[test]
    fn peaks_that_refine_to_different_bins_are_all_reported() {
        let mut psd = vec![0.0; 40];
        psd[9..12].copy_from_slice(&[0.2, 1.0, 0.2]);
        psd[29..32].copy_from_slice(&[0.2, 0.9, 0.2]);
        assert_eq!(detect_peaks_with(&psd, 0.05, 3, 5), vec![10, 30]);
    }

    /// A triangle of half-width 15 on 200 bins, the shape the belt parameters (window 20,
    /// vicinity 15) see.
    fn triangle(centre: f64) -> Vec<f64> {
        (0..200)
            .map(|i| (1.0 - (i as f64 - centre).abs() / 15.0).max(0.0))
            .collect()
    }

    #[test]
    fn a_peak_near_the_start_is_refined_to_its_raw_maximum_for_belt_parameters() {
        assert_eq!(detect_peaks_with(&triangle(10.3), 0.05, 20, 15), vec![10]);
    }

    #[test]
    fn the_whole_curve_detector_finds_a_peak_in_the_last_half_window() {
        let psd = triangle(190.3);
        assert!(detect_peaks_with(&psd, 0.05, 20, 15).is_empty());
        assert_eq!(detect_peaks_whole(&psd, 0.05, 20, 15), vec![190]);
    }

    #[test]
    fn the_whole_curve_detector_finds_a_peak_in_the_first_bins() {
        let psd = triangle(7.3);
        assert!(detect_peaks_with(&psd, 0.05, 20, 15).is_empty());
        assert_eq!(detect_peaks_whole(&psd, 0.05, 20, 15), vec![7]);
    }

    #[test]
    fn the_whole_curve_detector_finds_a_symmetric_peak_on_a_bin() {
        let psd = triangle(100.0);
        assert!(detect_peaks_with(&psd, 0.05, 20, 15).is_empty());
        assert_eq!(detect_peaks_whole(&psd, 0.05, 20, 15), vec![100]);
    }

    #[test]
    fn the_whole_curve_detector_reports_a_two_bin_plateau_once() {
        let psd = triangle(100.5);
        assert_eq!(detect_peaks_whole(&psd, 0.05, 20, 15), vec![100]);
    }

    #[test]
    fn the_whole_curve_detector_finds_the_same_middle_peak_as_the_shaketune_one() {
        let psd = triangle(100.3);
        assert_eq!(
            detect_peaks_whole(&psd, 0.05, 20, 15),
            detect_peaks_with(&psd, 0.05, 20, 15)
        );
    }

    #[test]
    fn the_whole_curve_detector_handles_short_curves_and_odd_windows() {
        assert!(detect_peaks_whole(&[1.0, 2.0], 0.0, 20, 15).is_empty());
        assert_eq!(
            detect_peaks_whole(&triangle(100.3), 0.05, 21, 15),
            vec![100]
        );
        assert!(detect_peaks_whole(&triangle(100.3), 0.05, 0, 15).len() <= 1);
    }

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

    #[test]
    fn two_peaks_five_bins_apart_stay_two_peaks_on_the_graph() {
        let bump = |i: usize, c: f64| (-((i as f64 - c).powi(2)) / 4.5).exp();
        let psd: Vec<f64> = (0..500)
            .map(|i| bump(i, 200.0) + 0.9 * bump(i, 205.0))
            .collect();
        let data = data(psd);
        assert_eq!(psd_peaks_with(&data, 1000.0, 5).unwrap().peaks.len(), 1);
        assert_eq!(psd_peaks(&data, 1000.0).unwrap().peaks.len(), 2);
    }
}
