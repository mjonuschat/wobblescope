//! Comparison of two belts' PSDs, computed the way Shake&Tune's belt comparison computes it
//! (`belts_computation.py` and the offset table of `belts_plotter.py`, at the commit pinned in
//! `docs/specs/shaketune-deviations.md`).
//!
//! The port reproduces Shake&Tune's figures, quirks included, and a test covers each
//! behaviour. Where a result is a defect the port departs from it, and a test pins the
//! departure.

use serde::{Deserialize, Serialize};

use crate::numeric::{interp, linspace, median_sorted as median, percentile_sorted as percentile};
use crate::peaks::detect_peaks_whole;

pub const GRID_POINTS: usize = 500;
/// The lower end in Hz of the band read when a session records no sweep: Klipper's
/// default `min_freq`.
pub const SWEEP_FLOOR_HZ: f64 = 5.0;
/// A peak must rise above this fraction of its own curve's maximum.
pub const PEAK_THRESHOLD: f64 = 0.1;
/// Width of the moving average in PSD bins. A Hann window's main lobe is four bins wide,
/// so features closer than that are one in the data already; Shake&Tune's 20 grid points are
/// 8 Hz at `max_freq` 200 and a bin of Klipper's PSD is about 1.56 Hz, which is five bins.
pub const PEAK_WINDOW_BINS: f64 = 5.0;
/// Reach of the peak refinement in PSD bins; the same 3:4 ratio to the window as
/// Shake&Tune's 15 to 20.
pub const PEAK_VICINITY_BINS: f64 = 4.0;
/// Cap on the pairing cutoff in Hz, compared as written: a pair is accepted at
/// `distance <= cutoff`. Shake&Tune compares `distance < cutoff + 1`, which makes the
/// real cap 11 Hz.
pub const PAIR_CAP_HZ: f64 = 10.0;
/// Number of paired peaks above which the health score is scaled down.
pub const MAX_PEAKS: usize = 2;
/// Health points taken off for an unpaired peak as tall as the highest value of either curve.
pub const UNPAIRED_PENALTY: f64 = 30.0;
/// The imbalance figure at and above which the health score is not scaled; below it the score
/// is scaled by `figure / TOLERANCE`.
pub const TOLERANCE: f64 = 0.8;
/// Width in Hz of the windows over which the belts' power is compared.
pub const IMBALANCE_WINDOW_HZ: f64 = 10.0;
/// Consecutive windows with the same stronger belt that make an imbalance.
pub const IMBALANCE_RUN: usize = 3;
/// A window counts only when its stronger belt rises above this fraction of the median
/// window power.
pub const IMBALANCE_FLOOR: f64 = 0.05;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BeltPeak {
    pub index: usize,
    /// Frequency in Hz.
    pub freq: f64,
    /// The value of this belt's own curve at the peak.
    pub level: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BeltPair {
    pub a: BeltPeak,
    pub b: BeltPeak,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BeltCurve {
    /// The PSD on the common grid.
    pub psd: Vec<f64>,
    /// Grid indices of the peaks. May hold duplicates, as Shake&Tune's does.
    pub peaks: Vec<usize>,
    /// Grid indices of the peaks that found no partner, in order.
    pub unpaired: Vec<usize>,
}

/// One row of the offset table beside the overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PeakOffset {
    /// Distance in Hz between the two peaks of the pair.
    pub freq_delta: f64,
    /// Difference between belt B's value at its peak and belt A's value at its peak, as a
    /// percentage of the highest value of either curve inside the swept band, or from
    /// `SWEEP_FLOOR_HZ` up when the session recorded none.
    pub amplitude_delta_percent: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Belt {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ImbalanceState {
    Found,
    /// Enough windows to look at, and no such run. This does not claim the belts are balanced.
    NoneFound,
    /// Fewer than three windows above the floor, or a band under 30 Hz.
    Unavailable,
}

/// How much weaker one belt is than the other over the most lopsided stretch of the band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BeltImbalance {
    pub state: ImbalanceState,
    /// Median of the weaker over the stronger belt's power across the run's three windows,
    /// 0 to 1. 1 unless `state` is `Found`. It is not a bound on each window.
    pub figure: f64,
    /// Start and end in Hz of the run; 0 unless `state` is `Found`.
    pub lo_hz: f64,
    pub hi_hz: f64,
    /// The belt with more power over the run; `None` unless `state` is `Found`.
    pub stronger: Option<Belt>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BeltsResult {
    /// The common grid in Hz, `GRID_POINTS` values from 0 to `max_freq`.
    pub freqs: Vec<f64>,
    pub a: BeltCurve,
    pub b: BeltCurve,
    /// Peaks matched across the belts, by rising frequency of belt A's peak. The offset
    /// table and the scatter label them in this order.
    pub pairs: Vec<BeltPair>,
    pub offsets: Vec<PeakOffset>,
    /// Pearson correlation of the two curves times 100, clipped to 0..100, over the swept band
    /// when the session recorded it and from `SWEEP_FLOOR_HZ` up otherwise. 0 when a
    /// curve is flat.
    pub similarity: f64,
    /// The level imbalance between the belts, which scales the health score.
    pub imbalance: BeltImbalance,
    /// Mechanical health text: Shake&Tune's bands, applied to a score that the level
    /// imbalance scales by the imbalance figure.
    pub health: String,
    pub max_freq: f64,
}

/// Compares two PSDs. Each is cut at `max_freq` and resampled onto `GRID_POINTS` points from 0
/// to `max_freq`, and the peaks, pairs, similarity and health are worked out on that grid, from
/// `SWEEP_FLOOR_HZ` up: a call without a sweep is a session that recorded none.
pub fn compare_psds(
    freqs_a: &[f64],
    psd_a: &[f64],
    freqs_b: &[f64],
    psd_b: &[f64],
    max_freq: f64,
) -> BeltsResult {
    compare_psds_in_band(
        freqs_a,
        psd_a,
        freqs_b,
        psd_b,
        max_freq,
        Some((SWEEP_FLOOR_HZ, max_freq)),
    )
}

/// As [`compare_psds`], with the similarity, the peaks, the offset and penalty normalization
/// and the imbalance windows taken only inside `sweep`, the `(FREQ_START, FREQ_END)` of the
/// test, ends included. `None` uses the whole grid, which is
/// all Shake&Tune does; [`compare_psds`] and the belt analysis pass the 5 Hz floor instead
/// when no sweep is recorded.
pub fn compare_psds_in_band(
    freqs_a: &[f64],
    psd_a: &[f64],
    freqs_b: &[f64],
    psd_b: &[f64],
    max_freq: f64,
    sweep: Option<(f64, f64)>,
) -> BeltsResult {
    let grid = linspace(0.0, max_freq, GRID_POINTS);
    // Both curves are smoothed over the same width in Hz, that of the coarser PSD.
    let bin = bin_spacing(freqs_a).max(bin_spacing(freqs_b));
    let (psd_a, peaks_a) = resample_and_find_peaks(&grid, freqs_a, psd_a, max_freq, bin, sweep);
    let (psd_b, peaks_b) = resample_and_find_peaks(&grid, freqs_b, psd_b, max_freq, bin, sweep);

    let (mut index_pairs, unpaired_a, unpaired_b) = pair_peaks(&peaks_a, &peaks_b, &grid);
    // Shake&Tune numbers the pairs in the order the greedy match found them (closest
    // first), so the first label is not the lowest peak. Grid indices rise with frequency.
    index_pairs.sort_unstable();
    // Out-of-band values would shrink every offset and penalty that is a share of this.
    let band_max = |psd: &[f64]| {
        let values: Vec<f64> = grid
            .iter()
            .zip(psd)
            .filter(|(f, _)| sweep.is_none_or(|(lo, hi)| **f >= lo && **f <= hi))
            .map(|(_, v)| *v)
            .collect();
        nan_max(f64::NEG_INFINITY, &values)
    };
    let highest = nan_max(band_max(&psd_a), &[band_max(&psd_b)]);

    let pairs: Vec<BeltPair> = index_pairs
        .iter()
        .map(|&(i, j)| BeltPair {
            a: BeltPeak {
                index: i,
                freq: grid[i],
                level: psd_a[i],
            },
            b: BeltPeak {
                index: j,
                freq: grid[j],
                level: psd_b[j],
            },
        })
        .collect();
    let offsets = index_pairs
        .iter()
        .map(|&(i, j)| PeakOffset {
            freq_delta: (grid[j] - grid[i]).abs(),
            // Shake&Tune reads belt A's curve at both peaks, so two peaks in one bin always
            // give 0. The amplitude of belt B's peak is read from belt B.
            amplitude_delta_percent: ((psd_b[j] - psd_a[i]) / highest).abs() * 100.0,
        })
        .collect();

    // A constant curve has no variance, so the correlation is NaN, which Shake&Tune's clip
    // keeps. A dead or saturated chip scores 0.
    let in_band: Vec<usize> = (0..grid.len())
        .filter(|&i| sweep.is_none_or(|(lo, hi)| grid[i] >= lo && grid[i] <= hi))
        .collect();
    let band_a: Vec<f64> = in_band.iter().map(|&i| psd_a[i]).collect();
    let band_b: Vec<f64> = in_band.iter().map(|&i| psd_b[i]).collect();
    // Outside the sweep nothing excites the machine, so those points are noise.
    let r = if in_band.len() < 2 {
        f64::NAN
    } else {
        correlation(&band_a, &band_b)
    };
    let similarity = if r.is_nan() {
        0.0
    } else {
        (r * 100.0).clamp(0.0, 100.0)
    };
    let a = BeltCurve {
        psd: psd_a,
        peaks: peaks_a,
        unpaired: unpaired_a,
    };
    let b = BeltCurve {
        psd: psd_b,
        peaks: peaks_b,
        unpaired: unpaired_b,
    };
    let imbalance = window_imbalance(&grid, &a.psd, &b.psd, sweep);
    let health = mechanical_health(similarity, imbalance.figure, &a, &b, pairs.len(), highest);
    BeltsResult {
        freqs: grid,
        a,
        b,
        pairs,
        offsets,
        similarity,
        imbalance,
        health,
        max_freq,
    }
}

fn resample_and_find_peaks(
    grid: &[f64],
    freqs: &[f64],
    psd: &[f64],
    max_freq: f64,
    bin: f64,
    sweep: Option<(f64, f64)>,
) -> (Vec<f64>, Vec<usize>) {
    let (cut_freqs, cut_psd): (Vec<f64>, Vec<f64>) = freqs
        .iter()
        .zip(psd)
        .filter(|(f, _)| **f <= max_freq)
        .map(|(f, p)| (*f, *p))
        .unzip();
    let resampled = interp(grid, &cut_freqs, &cut_psd);
    // A peak outside the sweep is noise: it would become an unpaired peak and cost health points
    // that the similarity, read over the band, never sees. Its height must not set the
    // threshold either, or a tall one hides the real peaks inside the band.
    let in_band = |f: f64| sweep.is_none_or(|(lo, hi)| f >= lo && f <= hi);
    let band: Vec<f64> = grid
        .iter()
        .zip(&resampled)
        .filter(|(f, _)| in_band(**f))
        .map(|(_, v)| *v)
        .collect();
    let band_max = nan_max(f64::NEG_INFINITY, &band);
    let threshold = PEAK_THRESHOLD * band_max;
    let step = (grid[grid.len() - 1] - grid[0]) / (grid.len() - 1) as f64;
    let points = |bins: f64| ((bins * bin / step).round() as usize).max(1);
    let mut peaks = detect_peaks_whole(
        &resampled,
        threshold,
        points(PEAK_WINDOW_BINS),
        points(PEAK_VICINITY_BINS),
    );
    peaks.retain(|&i| in_band(grid[i]));
    (resampled, peaks)
}

/// Median gap between neighbouring bins in Hz; 0 when there are fewer than two.
fn bin_spacing(freqs: &[f64]) -> f64 {
    let mut gaps: Vec<f64> = freqs.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort_by(f64::total_cmp);
    if gaps.is_empty() { 0.0 } else { median(&gaps) }
}

/// Greedy pairing, closest first, as `_pair_peaks`: matched index pairs, then the leftover
/// peaks of each belt. Like Python's `list.remove`, a matched index takes out only its first
/// occurrence, so duplicates of it stay behind as unpaired.
fn pair_peaks(
    peaks_a: &[usize],
    peaks_b: &[usize],
    grid: &[f64],
) -> (Vec<(usize, usize)>, Vec<usize>, Vec<usize>) {
    let mut distances: Vec<f64> = peaks_a
        .iter()
        .flat_map(|&p| peaks_b.iter().map(move |&q| (grid[p] - grid[q]).abs()))
        .collect();
    let threshold = if distances.is_empty() {
        PAIR_CAP_HZ
    } else {
        distances.sort_by(f64::total_cmp);
        let iqr = percentile(&distances, 75.0) - percentile(&distances, 25.0);
        (median(&distances) + 1.5 * iqr).min(PAIR_CAP_HZ)
    };

    let mut pairs = Vec::new();
    let mut left_a = peaks_a.to_vec();
    let mut left_b = peaks_b.to_vec();
    while !left_a.is_empty() && !left_b.is_empty() {
        let mut min_distance = f64::INFINITY;
        let mut best = None;
        for &p in &left_a {
            for &q in &left_b {
                let distance = (grid[p] - grid[q]).abs();
                if distance <= threshold && distance < min_distance {
                    min_distance = distance;
                    best = Some((p, q));
                }
            }
        }
        let Some((p, q)) = best else { break };
        pairs.push((p, q));
        remove_first(&mut left_a, p);
        remove_first(&mut left_b, q);
    }
    (pairs, left_a, left_b)
}

fn remove_first(list: &mut Vec<usize>, value: usize) {
    if let Some(at) = list.iter().position(|&v| v == value) {
        list.remove(at);
    }
}

/// Finds the most lopsided stretch of the band. Shake&Tune's similarity is a correlation,
/// so one belt carrying twice the other's power still scores 99 %.
///
/// The band, `band` or the whole grid when `None`, cut at the grid's end, is split into
/// consecutive 10 Hz windows from its lower limit; a trailing piece shorter than that is
/// dropped. A belt's window power is the sum of its values at grid points `lo <= f < hi`.
/// A window is eligible when its stronger belt exceeds 5 % of the median over all windows of
/// the stronger belt's power, so a large low-frequency resonance cannot raise the floor. A run
/// is three consecutive eligible windows with the same stronger belt; the run with the lowest
/// median ratio wins, the lowest frequency on a tie. With no run the figure is 1, and with
/// fewer than three eligible windows or a band under 30 Hz the result is `Unavailable`, also
/// with figure 1.
///
/// Limitation: a wide feature centred on a window edge can lift three neighbouring windows
/// and read as an imbalance, and a gap that is narrower than 30 Hz or not aligned to the
/// windows is not guaranteed to be caught. An imbalance is caught when three consecutive
/// 10 Hz windows each have the same stronger belt and sit above the floor; whether a narrower
/// or misaligned gap meets that depends on how it falls across the windows.
///
/// The run rule is all or nothing: if one of a run's three
/// windows falls below the floor, ties or reverses, the run is gone and the score is unscaled,
/// so a small input change can lift the score.
fn window_imbalance(
    grid: &[f64],
    psd_a: &[f64],
    psd_b: &[f64],
    band: Option<(f64, f64)>,
) -> BeltImbalance {
    let unscaled = |state| BeltImbalance {
        state,
        figure: 1.0,
        lo_hz: 0.0,
        hi_hz: 0.0,
        stronger: None,
    };
    let Some(&grid_end) = grid.last() else {
        return unscaled(ImbalanceState::Unavailable);
    };
    let (start, end) = match band {
        Some((lo, hi)) => (lo, hi.min(grid_end)),
        None => (grid[0], grid_end),
    };
    let span = end - start;
    if span.is_nan() || span < IMBALANCE_RUN as f64 * IMBALANCE_WINDOW_HZ {
        return unscaled(ImbalanceState::Unavailable);
    }

    struct Window {
        lo: f64,
        stronger: Option<Belt>,
        strong: f64,
        ratio: f64,
        eligible: bool,
    }
    let count = (span / IMBALANCE_WINDOW_HZ).floor() as usize;
    let mut windows: Vec<Window> = (0..count)
        .map(|k| {
            // Both edges are computed from `start`, so a shared edge is the same value.
            let lo = start + k as f64 * IMBALANCE_WINDOW_HZ;
            let hi = start + (k + 1) as f64 * IMBALANCE_WINDOW_HZ;
            let power = |psd: &[f64]| -> f64 {
                grid.iter()
                    .zip(psd)
                    .filter(|(f, _)| **f >= lo && **f < hi)
                    .map(|(_, v)| *v)
                    .sum()
            };
            let (a, b) = (power(psd_a), power(psd_b));
            let (stronger, strong, weak) = if a > b {
                (Some(Belt::A), a, b)
            } else if b > a {
                (Some(Belt::B), b, a)
            } else {
                (None, a, b)
            };
            Window {
                lo,
                stronger,
                strong,
                ratio: if strong > 0.0 { weak / strong } else { 1.0 },
                eligible: false,
            }
        })
        .collect();
    let mut sorted: Vec<f64> = windows.iter().map(|w| w.strong).collect();
    sorted.sort_by(f64::total_cmp);
    let floor = IMBALANCE_FLOOR * median(&sorted);
    for w in &mut windows {
        w.eligible = w.strong > floor;
    }
    if windows.iter().filter(|w| w.eligible).count() < IMBALANCE_RUN {
        return unscaled(ImbalanceState::Unavailable);
    }

    let mut best: Option<(f64, usize)> = None;
    for (k, run) in windows.windows(IMBALANCE_RUN).enumerate() {
        let belt = run[0].stronger;
        if belt.is_none() || !run.iter().all(|w| w.eligible && w.stronger == belt) {
            continue;
        }
        let mut ratios: Vec<f64> = run.iter().map(|w| w.ratio).collect();
        ratios.sort_by(f64::total_cmp);
        let figure = median(&ratios);
        if best.is_none_or(|(lowest, _)| figure < lowest) {
            best = Some((figure, k));
        }
    }
    match best {
        Some((figure, k)) => BeltImbalance {
            state: ImbalanceState::Found,
            figure,
            lo_hz: windows[k].lo,
            hi_hz: windows[k].lo + IMBALANCE_RUN as f64 * IMBALANCE_WINDOW_HZ,
            stronger: windows[k].stronger,
        },
        None => unscaled(ImbalanceState::NoneFound),
    }
}

/// `_compute_mhi` and `_mhi_lut`, with the similarity scaled by the level imbalance multiplier,
/// `min(1, figure / TOLERANCE)`. A NaN score reads as unknown.
fn mechanical_health(
    similarity: f64,
    figure: f64,
    a: &BeltCurve,
    b: &BeltCurve,
    pair_count: usize,
    highest: f64,
) -> String {
    let mut score = similarity * (figure / TOLERANCE).min(1.0);
    if pair_count >= MAX_PEAKS {
        score *= MAX_PEAKS as f64 / pair_count as f64;
    }
    let penalty: f64 = a
        .unpaired
        .iter()
        .map(|&p| a.psd[p])
        .chain(b.unpaired.iter().map(|&p| b.psd[p]))
        .fold(0.0, |sum, level| sum + level / highest * UNPAIRED_PENALTY);
    score -= penalty;
    health_text(score.clamp(0.0, 100.0))
}

/// Bands are open at the bottom, except the last, which includes 0: Shake&Tune's open
/// bottom leaves a score of exactly 0 matching no band, so the worst result read "Unknown".
fn health_text(score: f64) -> String {
    let text = if score > 70.0 {
        "Excellent mechanical health"
    } else if score > 55.0 {
        "Good mechanical health"
    } else if score > 45.0 {
        "Acceptable mechanical health"
    } else if score > 30.0 {
        "Potential signs of a mechanical issue"
    } else if score > 15.0 {
        "Likely a mechanical issue"
    } else if score >= 0.0 {
        "Mechanical issue detected"
    } else {
        "Unknown mechanical health"
    };
    text.to_owned()
}

/// Maximum that is NaN when any value is, as `ndarray.max`.
fn nan_max(init: f64, values: &[f64]) -> f64 {
    values.iter().fold(init, |m, &v| {
        if m.is_nan() || v.is_nan() {
            f64::NAN
        } else {
            m.max(v)
        }
    })
}

/// numpy's pairwise summation, so that means round as `ndarray.mean` does.
fn pairwise_sum(v: &[f64]) -> f64 {
    let n = v.len();
    if n < 8 {
        v.iter().fold(0.0, |s, x| s + x)
    } else if n <= 128 {
        let mut r: [f64; 8] = v[..8].try_into().expect("eight values");
        let mut i = 8;
        while i + 8 <= n - n % 8 {
            for (k, slot) in r.iter_mut().enumerate() {
                *slot += v[i + k];
            }
            i += 8;
        }
        let mut sum = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        for x in &v[i..] {
            sum += x;
        }
        sum
    } else {
        let mut half = n / 2;
        half -= half % 8;
        pairwise_sum(&v[..half]) + pairwise_sum(&v[half..])
    }
}

/// `numpy.corrcoef(x, y)[0, 1]`: NaN when either curve is constant.
fn correlation(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let mean_x = pairwise_sum(x) / n;
    let mean_y = pairwise_sum(y) / n;
    let dx: Vec<f64> = x.iter().map(|v| v - mean_x).collect();
    let dy: Vec<f64> = y.iter().map(|v| v - mean_y).collect();
    let dot = |p: &[f64], q: &[f64]| {
        let products: Vec<f64> = p.iter().zip(q).map(|(a, b)| a * b).collect();
        pairwise_sum(&products) / (n - 1.0)
    };
    let c = dot(&dx, &dy) / dot(&dx, &dx).sqrt() / dot(&dy, &dy).sqrt();
    c.clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_FREQ: f64 = 100.0;

    /// A PSD on 0..=250 Hz at Klipper's 1.5625 Hz bins with a Gaussian of 2 Hz width at each
    /// `(freq, height)`.
    fn modes(modes: &[(f64, f64)]) -> (Vec<f64>, Vec<f64>) {
        modes_at(1.5625, modes)
    }

    fn modes_at(step: f64, modes: &[(f64, f64)]) -> (Vec<f64>, Vec<f64>) {
        let freqs: Vec<f64> = (0..=(250.0 / step) as usize)
            .map(|i| i as f64 * step)
            .collect();
        let psd = freqs
            .iter()
            .map(|f| {
                modes
                    .iter()
                    .map(|(at, h)| h * (-((f - at) / 2.0).powi(2) / 2.0).exp())
                    .sum()
            })
            .collect();
        (freqs, psd)
    }

    fn compare(a: &[(f64, f64)], b: &[(f64, f64)], max_freq: f64) -> BeltsResult {
        let (fa, pa) = modes(a);
        let (fb, pb) = modes(b);
        compare_psds(&fa, &pa, &fb, &pb, max_freq)
    }

    fn curve(peaks: &[usize], unpaired: &[usize]) -> BeltCurve {
        BeltCurve {
            psd: vec![1.0; 10],
            peaks: peaks.to_vec(),
            unpaired: unpaired.to_vec(),
        }
    }

    #[test]
    fn identical_curves_score_100() {
        let got = compare(
            &[(50.0, 1.0), (80.0, 0.5)],
            &[(50.0, 1.0), (80.0, 0.5)],
            MAX_FREQ,
        );
        assert!((got.similarity - 100.0).abs() < 1e-9);
    }

    #[test]
    fn scaling_one_curve_by_five_still_scores_100() {
        let got = compare(
            &[(50.0, 1.0), (80.0, 0.5)],
            &[(50.0, 5.0), (80.0, 2.5)],
            MAX_FREQ,
        );
        assert!((got.similarity - 100.0).abs() < 1e-9);
    }

    #[test]
    fn an_unpaired_peak_lowers_the_health_text() {
        let got = compare(&[(50.0, 1.0), (80.0, 1.0)], &[(50.0, 1.0)], MAX_FREQ);
        assert_eq!(got.a.unpaired.len(), 1);
        assert_eq!(
            unscaled_health(&got),
            "Potential signs of a mechanical issue"
        );
        // The imbalance check also sees that belt B has no power around 80 Hz, which
        // lowers the text further.
        assert_eq!(got.imbalance.state, ImbalanceState::Found);
        assert_eq!(got.health, "Mechanical issue detected");
    }

    #[test]
    fn the_amplitude_delta_of_peaks_in_one_bin_reads_both_belts() {
        // Both peaks at 50 Hz, belt B five times as tall: |5 - 1| / 5 is 80%.
        let got = compare(&[(50.0, 1.0)], &[(50.0, 5.0)], MAX_FREQ);
        assert_eq!(got.offsets.len(), 1);
        assert!((got.offsets[0].amplitude_delta_percent - 80.0).abs() < 1e-6);
    }

    #[test]
    fn the_amplitude_delta_reads_belt_b_at_its_peak_and_belt_a_at_its_own() {
        // Belt A has a tall peak at 50 Hz and a short one at 56 Hz, which belt B merges into
        // one peak at 56 Hz. The pair is A's peak at 50 Hz with B's at 56 Hz.
        let got = compare(&[(50.0, 1.0), (56.0, 0.4)], &[(56.0, 3.0)], MAX_FREQ);
        assert_eq!(got.pairs.len(), 1);
        let pair = &got.pairs[0];
        let (i, j) = (pair.a.index, pair.b.index);
        assert_ne!(i, j);
        let highest = got
            .a
            .psd
            .iter()
            .chain(&got.b.psd)
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let want_amplitude = ((got.b.psd[j] - got.a.psd[i]) / highest * 100.0).abs();
        let want_freq = (got.freqs[j] - got.freqs[i]).abs();
        assert!(want_amplitude > 1.0);
        assert!((got.offsets[0].freq_delta - want_freq).abs() < 1e-12);
        assert!((got.offsets[0].amplitude_delta_percent - want_amplitude).abs() < 1e-9);
        // Shake&Tune's figure, belt A at both peaks, is another number.
        let from_a = ((got.a.psd[j] - got.a.psd[i]) / highest * 100.0).abs();
        assert!((got.offsets[0].amplitude_delta_percent - from_a).abs() > 1.0);
    }

    #[test]
    fn pairs_and_offsets_are_listed_by_rising_frequency() {
        // The 80 Hz pair matches exactly, so the greedy match finds it first; the 50 Hz pair,
        // 2 Hz apart, second.
        let got = compare(
            &[(50.0, 1.0), (80.0, 1.0)],
            &[(52.0, 1.0), (80.0, 1.0)],
            MAX_FREQ,
        );
        assert_eq!(got.pairs.len(), 2);
        assert!(got.pairs[0].a.freq < got.pairs[1].a.freq);
        assert!(got.pairs[0].b.freq < got.pairs[1].b.freq);
        assert!(
            got.offsets[0].freq_delta > 1.0,
            "the 50 Hz pair comes first"
        );
        assert!(got.offsets[1].freq_delta < 1.0);
    }

    #[test]
    fn the_peak_count_does_not_depend_on_max_freq() {
        // Modes 7 Hz apart. Shake&Tune's 20-point window is 4 Hz wide at 100 Hz and 8 Hz
        // wide at 200 Hz, so it finds two peaks at 100 and one at 200.
        let both = [(50.0, 1.0), (57.0, 1.0)];
        let at = |max_freq| compare(&both, &both, max_freq).a.peaks.len();
        assert_eq!(at(100.0), at(200.0));
        assert_eq!(at(200.0), 1);
    }

    #[test]
    fn the_peak_window_follows_the_psd_bin_spacing() {
        // The same two modes 7 Hz apart. At 1.56 Hz bins the window is 7.8 Hz and they
        // merge; at 0.5 Hz bins it is 2.5 Hz and they stay apart.
        let psd = |step: f64| {
            let freqs: Vec<f64> = (0..)
                .map(|i| i as f64 * step)
                .take_while(|f| *f <= 250.0)
                .collect();
            let values = freqs
                .iter()
                .map(|f| {
                    (-((f - 50.0) / 1.0).powi(2) / 2.0).exp()
                        + (-((f - 57.0) / 1.0).powi(2) / 2.0).exp()
                })
                .collect::<Vec<f64>>();
            (freqs, values)
        };
        let count = |step| {
            let (f, p) = psd(step);
            compare_psds(&f, &p, &f, &p, 200.0).a.peaks.len()
        };
        assert_eq!(count(1.5625), 1);
        assert_eq!(count(0.5), 2);
    }

    #[test]
    fn the_pairing_cap_is_ten_hz_as_written() {
        // Shake&Tune pairs up to 11 Hz apart; we stop at the stated 10.
        // Fine bins, so that the peaks sit where the modes are.
        let at = |other: f64| {
            let (fa, pa) = modes_at(0.25, &[(40.0, 1.0)]);
            let (fb, pb) = modes_at(0.25, &[(other, 1.0)]);
            compare_psds(&fa, &pa, &fb, &pb, MAX_FREQ).pairs.len()
        };
        assert_eq!(at(50.0), 1);
        assert_eq!(at(50.5), 0);
    }

    #[test]
    fn similarity_reads_the_swept_band_only() {
        let (fa, pa) = modes(&[(50.0, 1.0), (90.0, 0.5)]);
        let (fb, pb) = modes(&[(50.0, 1.0), (90.0, 0.5), (170.0, 3.0)]);
        let whole = compare_psds_in_band(&fa, &pa, &fb, &pb, 200.0, None);
        let band = compare_psds_in_band(&fa, &pa, &fb, &pb, 200.0, Some((20.0, 120.0)));
        assert!(whole.similarity < 80.0, "{}", whole.similarity);
        assert!(band.similarity > 99.0, "{}", band.similarity);
        assert_eq!(band.freqs, whole.freqs, "only the figure changes");
    }

    #[test]
    fn peaks_outside_the_swept_band_are_not_counted() {
        // Belt B has a mode at 170 Hz that the sweep to 120 Hz never excited.
        let (fa, pa) = modes(&[(50.0, 1.0)]);
        let (fb, pb) = modes(&[(50.0, 1.0), (170.0, 1.0)]);
        let whole = compare_psds_in_band(&fa, &pa, &fb, &pb, 200.0, None);
        assert_eq!(whole.b.unpaired.len(), 1);
        let band = compare_psds_in_band(&fa, &pa, &fb, &pb, 200.0, Some((20.0, 120.0)));
        assert_eq!(band.b.peaks.len(), 1);
        assert!(band.b.unpaired.is_empty());
        assert_eq!(band.pairs.len(), 1);
        assert_ne!(whole.health, "Excellent mechanical health");
        assert_eq!(band.health, "Excellent mechanical health");
    }

    #[test]
    fn a_tall_peak_outside_the_band_does_not_hide_one_inside() {
        let (fa, pa) = modes(&[(50.0, 1.0)]);
        let (fb, pb) = modes(&[(50.0, 1.0), (170.0, 20.0)]);
        let got = compare_psds_in_band(&fa, &pa, &fb, &pb, 200.0, Some((20.0, 120.0)));
        assert_eq!(got.b.peaks.len(), 1);
        assert_eq!(got.pairs.len(), 1);
    }

    #[test]
    fn a_tall_peak_outside_the_band_does_not_shrink_the_offsets_and_penalty() {
        let (fa, pa) = modes(&[(50.0, 1.0)]);
        let (fb, pb) = modes(&[(50.0, 0.5), (170.0, 20.0)]);
        let band = Some((20.0, 120.0));
        let got = compare_psds_in_band(&fa, &pa, &fb, &pb, 200.0, band);
        let delta = got.offsets[0].amplitude_delta_percent;
        assert!((delta - 50.0).abs() < 1.0, "{delta}");
        // The unpaired peak costs the same with or without the out-of-band mode.
        let (fc, pc) = modes(&[(50.0, 1.0), (100.0, 1.0)]);
        let (fd, pd) = modes(&[(50.0, 1.0), (170.0, 20.0)]);
        let (fe, pe) = modes(&[(50.0, 1.0)]);
        let tall = compare_psds_in_band(&fc, &pc, &fd, &pd, 200.0, band);
        let clean = compare_psds_in_band(&fc, &pc, &fe, &pe, 200.0, band);
        assert_eq!(tall.a.unpaired.len(), 1);
        assert_eq!(tall.health, clean.health);
    }

    #[test]
    fn a_peak_on_the_band_edge_is_kept() {
        let (fa, pa) = modes(&[(60.0, 1.0)]);
        let at = compare_psds_in_band(&fa, &pa, &fa, &pa, 200.0, None).pairs[0]
            .a
            .freq;
        let kept = compare_psds_in_band(&fa, &pa, &fa, &pa, 200.0, Some((at, 120.0)));
        assert_eq!(kept.pairs.len(), 1);
        let dropped = compare_psds_in_band(&fa, &pa, &fa, &pa, 200.0, Some((at + 0.01, 120.0)));
        assert!(dropped.pairs.is_empty());
    }

    #[test]
    fn a_band_with_no_grid_point_scores_zero() {
        let (fa, pa) = modes(&[(50.0, 1.0)]);
        let got = compare_psds_in_band(&fa, &pa, &fa, &pa, MAX_FREQ, Some((300.0, 400.0)));
        assert_eq!(got.similarity, 0.0);
    }

    #[test]
    fn a_flat_curve_scores_zero_similarity() {
        let freqs: Vec<f64> = (0..=400).map(|i| i as f64 * 0.25).collect();
        let flat = vec![1.0; freqs.len()];
        let (fb, pb) = modes(&[(50.0, 1.0)]);
        let got = compare_psds(&freqs, &flat, &fb, &pb, MAX_FREQ);
        assert_eq!(got.similarity, 0.0);
    }

    #[test]
    fn two_flat_curves_score_zero_similarity() {
        let freqs: Vec<f64> = (0..=400).map(|i| i as f64 * 0.25).collect();
        let flat = vec![1.0; freqs.len()];
        let got = compare_psds(&freqs, &flat, &freqs, &flat, MAX_FREQ);
        assert_eq!(got.similarity, 0.0);
    }

    #[test]
    fn an_anticorrelated_pair_reads_as_an_issue_not_unknown() {
        let got = compare(&[(30.0, 1.0)], &[(80.0, 1.0)], MAX_FREQ);
        assert_eq!(got.similarity, 0.0);
        assert_eq!(got.health, "Mechanical issue detected");
    }

    #[test]
    fn swapping_the_belts_keeps_the_similarity_and_swaps_the_peak_lists() {
        let one = [(50.0, 1.0), (80.0, 1.0)];
        let two = [(50.0, 1.0)];
        let forward = compare(&one, &two, MAX_FREQ);
        let swapped = compare(&two, &one, MAX_FREQ);
        assert!((forward.similarity - swapped.similarity).abs() < 1e-9);
        assert_eq!(forward.a.peaks, swapped.b.peaks);
        assert_eq!(forward.b.peaks, swapped.a.peaks);
        assert_eq!(forward.a.unpaired, swapped.b.unpaired);
        assert_eq!(forward.b.unpaired, swapped.a.unpaired);
    }

    #[test]
    fn every_health_band_is_open_at_the_bottom_and_closed_at_the_top_but_the_last_includes_zero() {
        let cases = [
            (100.0, "Excellent mechanical health"),
            (70.5, "Excellent mechanical health"),
            (70.0, "Good mechanical health"),
            (55.5, "Good mechanical health"),
            (55.0, "Acceptable mechanical health"),
            (45.5, "Acceptable mechanical health"),
            (45.0, "Potential signs of a mechanical issue"),
            (30.5, "Potential signs of a mechanical issue"),
            (30.0, "Likely a mechanical issue"),
            (15.5, "Likely a mechanical issue"),
            (15.0, "Mechanical issue detected"),
            (0.5, "Mechanical issue detected"),
            (0.0, "Mechanical issue detected"),
            (f64::NAN, "Unknown mechanical health"),
        ];
        for (score, text) in cases {
            assert_eq!(health_text(score), text, "score {score}");
        }
    }

    #[test]
    fn health_scales_down_for_more_than_two_pairs() {
        let a = curve(&[], &[]);
        let b = curve(&[], &[]);
        assert_eq!(
            mechanical_health(80.0, 1.0, &a, &b, 2, 1.0),
            "Excellent mechanical health"
        );
        assert_eq!(
            mechanical_health(80.0, 1.0, &a, &b, 4, 1.0),
            "Potential signs of a mechanical issue"
        );
    }

    #[test]
    fn health_takes_30_points_for_an_unpaired_peak_as_tall_as_the_highest() {
        let a = curve(&[3], &[3]);
        let b = curve(&[], &[]);
        assert_eq!(
            mechanical_health(100.0, 1.0, &a, &b, 0, 1.0),
            "Good mechanical health"
        );
        assert_eq!(
            mechanical_health(80.0, 1.0, &a, &b, 0, 1.0),
            "Acceptable mechanical health"
        );
        let both = curve(&[3], &[3]);
        assert_eq!(
            mechanical_health(100.0, 1.0, &a, &both, 0, 1.0),
            "Potential signs of a mechanical issue"
        );
        assert_eq!(
            mechanical_health(50.0, 1.0, &a, &both, 0, 1.0),
            "Mechanical issue detected"
        );
    }

    #[test]
    fn duplicate_peaks_leave_their_copy_unpaired() {
        let grid = linspace(0.0, 100.0, GRID_POINTS);
        let (pairs, left_a, left_b) = pair_peaks(&[10, 10], &[10], &grid);
        assert_eq!(pairs, vec![(10, 10)]);
        assert_eq!(left_a, vec![10]);
        assert!(left_b.is_empty());
    }

    #[test]
    fn numpy_percentile_and_median_interpolate_linearly() {
        let v = [1.0, 2.0, 4.0, 8.0];
        assert_eq!(median(&v), 3.0);
        assert_eq!(percentile(&v, 25.0), 1.75);
        assert_eq!(percentile(&v, 75.0), 5.0);
    }

    #[test]
    fn a_flat_curve_serialises_a_zero_similarity() {
        let (f, p) = modes(&[(50.0, 1.0)]);
        let flat = vec![1.0; f.len()];
        let got = compare_psds(&f, &flat, &f, &p, MAX_FREQ);
        let json = serde_json::to_value(&got).expect("serialises");
        assert_eq!(json["similarity"], 0.0);
    }

    const BAND: Option<(f64, f64)> = Some((5.0, 100.0));

    fn grid_at(step: f64) -> Vec<f64> {
        (0..=(100.0 / step) as usize)
            .map(|i| i as f64 * step)
            .collect()
    }

    fn values(grid: &[f64], f: impl Fn(f64) -> f64) -> Vec<f64> {
        grid.iter().map(|&x| f(x)).collect()
    }

    fn imbalance_of_b(b: impl Fn(f64) -> f64) -> BeltImbalance {
        let grid = grid_at(0.5);
        let psd_a = vec![1.0; grid.len()];
        window_imbalance(&grid, &psd_a, &values(&grid, b), BAND)
    }

    fn in_range(f: f64, lo: f64, hi: f64) -> bool {
        f >= lo && f < hi
    }

    fn twice_over(lo: f64, hi: f64) -> impl Fn(f64) -> f64 {
        move |f| if in_range(f, lo, hi) { 2.0 } else { 1.0 }
    }

    fn score(similarity: f64, figure: f64) -> String {
        let none = curve(&[], &[]);
        mechanical_health(similarity, figure, &none, &none, 0, 1.0)
    }

    fn highest_of(got: &BeltsResult) -> f64 {
        got.a
            .psd
            .iter()
            .chain(&got.b.psd)
            .copied()
            .fold(0.0, f64::max)
    }

    fn unscaled_health(got: &BeltsResult) -> String {
        let highest = highest_of(got);
        mechanical_health(
            got.similarity,
            1.0,
            &got.a,
            &got.b,
            got.pairs.len(),
            highest,
        )
    }

    /// Compares two PSDs given as functions of frequency on 0..=250 Hz at 0.25 Hz bins.
    fn compare_fn(
        a: impl Fn(f64) -> f64,
        b: impl Fn(f64) -> f64,
        sweep: Option<(f64, f64)>,
    ) -> BeltsResult {
        let freqs: Vec<f64> = (0..=1000).map(|i| i as f64 * 0.25).collect();
        let (pa, pb) = (values(&freqs, a), values(&freqs, b));
        compare_psds_in_band(&freqs, &pa, &freqs, &pb, MAX_FREQ, sweep)
    }

    fn assert_unavailable_and_unscaled(got: &BeltsResult) {
        assert_eq!(got.imbalance.state, ImbalanceState::Unavailable);
        assert_eq!(got.imbalance.figure, 1.0);
        assert_eq!(got.health, unscaled_health(got));
        // A multiplier of one half would have changed the verdict.
        let highest = highest_of(got);
        let halved = mechanical_health(
            got.similarity,
            0.5,
            &got.a,
            &got.b,
            got.pairs.len(),
            highest,
        );
        assert_ne!(got.health, halved);
    }

    #[test]
    fn equal_curves_give_figure_one_and_an_unchanged_score() {
        let got = compare(
            &[(50.0, 1.0), (80.0, 0.5)],
            &[(50.0, 1.0), (80.0, 0.5)],
            MAX_FREQ,
        );
        assert_eq!(got.imbalance.state, ImbalanceState::NoneFound);
        assert_eq!(got.imbalance.figure, 1.0);
        assert_eq!(got.health, unscaled_health(&got));
        assert_eq!(got.health, "Excellent mechanical health");
    }

    #[test]
    fn two_x_over_three_windows_is_found() {
        let got = imbalance_of_b(twice_over(45.0, 75.0));
        assert_eq!(got.state, ImbalanceState::Found);
        assert!((got.figure - 0.5).abs() < 1e-12, "{}", got.figure);
        assert_eq!((got.lo_hz, got.hi_hz), (45.0, 75.0));
        assert_eq!(got.stronger, Some(Belt::B));
    }

    #[test]
    fn a_twenty_hz_gap_in_two_windows_is_not_a_run() {
        let got = imbalance_of_b(twice_over(45.0, 65.0));
        assert_eq!(got.state, ImbalanceState::NoneFound);
        assert_eq!(got.figure, 1.0);
    }

    #[test]
    fn a_thirty_hz_gap_shifted_by_five_hz_gives_what_the_window_powers_give() {
        // B is 2x over 50 to 80 Hz. The windows 45 to 55 and 75 to 85 hold half of it, so
        // their ratios are 2/3 and the middle two are 1/2. Two runs have the median 1/2 and
        // the lower one wins.
        let got = imbalance_of_b(twice_over(50.0, 80.0));
        assert_eq!(got.state, ImbalanceState::Found);
        assert!((got.figure - 0.5).abs() < 1e-12, "{}", got.figure);
        assert_eq!((got.lo_hz, got.hi_hz), (45.0, 75.0));
        assert_eq!(got.stronger, Some(Belt::B));
    }

    #[test]
    fn the_figure_is_the_median_of_the_run() {
        // B is 0.9, 0.6 and 0.1 of A in the three windows. The ratio of the summed powers,
        // 1.6 / 3, is another number.
        let got = imbalance_of_b(|f| match f {
            f if in_range(f, 45.0, 55.0) => 0.9,
            f if in_range(f, 55.0, 65.0) => 0.6,
            f if in_range(f, 65.0, 75.0) => 0.1,
            _ => 1.0,
        });
        assert_eq!(got.state, ImbalanceState::Found);
        assert!((got.figure - 0.6).abs() < 1e-12, "{}", got.figure);
        assert!((got.figure - 1.6 / 3.0).abs() > 0.05);
        assert_eq!(got.stronger, Some(Belt::A));
    }

    #[test]
    fn a_one_bin_spike_and_a_shifted_peak_make_no_run() {
        let spike = imbalance_of_b(|f| if f == 60.0 { 100.0 } else { 1.0 });
        assert_eq!(spike.state, ImbalanceState::NoneFound);
        // A peak at 54.5 Hz in belt A and at 55.5 Hz in belt B: the stronger belt flips at the
        // edge between two windows.
        let grid = grid_at(0.5);
        let psd_a = values(&grid, |f| if f == 54.5 { 50.0 } else { 1.0 });
        let psd_b = values(&grid, |f| if f == 55.5 { 50.0 } else { 1.0 });
        let shifted = window_imbalance(&grid, &psd_a, &psd_b, BAND);
        assert_eq!(shifted.state, ImbalanceState::NoneFound);
        assert_eq!(shifted.figure, 1.0);
    }

    #[test]
    fn a_run_broken_by_a_reversal_or_a_tie_is_not_counted() {
        let reversal = imbalance_of_b(|f| match f {
            f if in_range(f, 45.0, 65.0) => 2.0,
            f if in_range(f, 65.0, 75.0) => 0.5,
            _ => 1.0,
        });
        assert_eq!(reversal.state, ImbalanceState::NoneFound);
        let tie = imbalance_of_b(|f| {
            if in_range(f, 45.0, 65.0) || in_range(f, 75.0, 85.0) {
                2.0
            } else {
                1.0
            }
        });
        assert_eq!(tie.state, ImbalanceState::NoneFound);
    }

    #[test]
    fn a_point_on_a_window_edge_belongs_to_the_upper_window() {
        for step in [0.5, 0.25] {
            let grid = grid_at(step);
            let psd_a = vec![1.0; grid.len()];
            // B is stronger from exactly 55 Hz, an edge of the windows that start at 5 Hz.
            let psd_b = values(&grid, |f| if f >= 55.0 { 2.0 } else { 1.0 });
            let got = window_imbalance(&grid, &psd_a, &psd_b, BAND);
            assert_eq!(got.state, ImbalanceState::Found, "step {step}");
            assert_eq!((got.lo_hz, got.hi_hz), (55.0, 85.0), "step {step}");
        }
    }

    #[test]
    fn a_grid_point_next_to_a_shared_edge_is_counted_in_exactly_one_window() {
        // For a start of 7.874, (start + 20) + 10 is one ulp under start + 30. A window edge
        // built from the previous one would leave a grid point at that value in no window.
        let start = 7.874;
        let almost_edge = (start + 20.0) + 10.0;
        assert!(almost_edge < start + 30.0);
        let mut grid = grid_at(0.5);
        grid.push(almost_edge);
        grid.sort_by(f64::total_cmp);
        let psd_a = vec![1.0; grid.len()];
        let psd_b = values(&grid, |f| if f >= almost_edge { 2.0 } else { 1.0 });
        let got = window_imbalance(&grid, &psd_a, &psd_b, Some((start, 100.0)));
        assert_eq!(got.state, ImbalanceState::Found);
        // The point is in the window that ends at the shared edge, so B is stronger there.
        assert_eq!(got.lo_hz, start + 20.0);
        assert!((got.hi_hz - (start + 50.0)).abs() < 1e-9);
    }

    #[test]
    fn a_big_low_frequency_resonance_does_not_hide_a_high_gap() {
        let grid = grid_at(0.5);
        let resonance = |f: f64| if in_range(f, 5.0, 15.0) { 1000.0 } else { 1.0 };
        let psd_a = values(&grid, resonance);
        let psd_b = values(&grid, |f| {
            resonance(f) * if in_range(f, 45.0, 75.0) { 2.0 } else { 1.0 }
        });
        let got = window_imbalance(&grid, &psd_a, &psd_b, BAND);
        assert_eq!(got.state, ImbalanceState::Found);
        assert_eq!((got.lo_hz, got.hi_hz), (45.0, 75.0));
    }

    #[test]
    fn one_zero_belt_window_has_ratio_zero_and_two_zero_ratios_give_figure_zero_and_score_zero() {
        let grid = grid_at(0.5);
        let psd_b = vec![1.0; grid.len()];
        let psd_a = values(&grid, |f| if in_range(f, 45.0, 75.0) { 0.0 } else { 1.0 });
        let got = window_imbalance(&grid, &psd_a, &psd_b, BAND);
        assert_eq!(got.state, ImbalanceState::Found);
        assert_eq!(got.figure, 0.0);
        assert_eq!(got.stronger, Some(Belt::B));
        assert_eq!(score(100.0, got.figure), "Mechanical issue detected");
        assert_eq!(score(100.0, 1.0), "Excellent mechanical health");
    }

    #[test]
    fn a_zero_median_run_has_figure_zero() {
        let grid = grid_at(0.5);
        let psd_b = vec![1.0; grid.len()];
        let psd_a = values(&grid, |f| match f {
            f if in_range(f, 45.0, 65.0) => 0.0,
            f if in_range(f, 65.0, 75.0) => 0.5,
            _ => 1.0,
        });
        let got = window_imbalance(&grid, &psd_a, &psd_b, BAND);
        assert_eq!(got.state, ImbalanceState::Found);
        assert_eq!(got.figure, 0.0);
        assert_eq!(score(100.0, got.figure), "Mechanical issue detected");
    }

    #[test]
    fn one_zero_window_with_two_nonzero_ratios_gives_a_nonzero_median() {
        let grid = grid_at(0.5);
        let psd_b = vec![1.0; grid.len()];
        let psd_a = values(&grid, |f| match f {
            f if in_range(f, 45.0, 55.0) => 0.0,
            f if in_range(f, 55.0, 75.0) => 0.5,
            _ => 1.0,
        });
        let got = window_imbalance(&grid, &psd_a, &psd_b, BAND);
        assert_eq!(got.state, ImbalanceState::Found);
        assert!((got.figure - 0.5).abs() < 1e-12, "{}", got.figure);
    }

    #[test]
    fn a_window_with_both_zero_is_skipped() {
        let grid = grid_at(0.5);
        let psd_a = values(&grid, |f| if in_range(f, 55.0, 65.0) { 0.0 } else { 1.0 });
        let psd_b = values(&grid, |f| match f {
            f if in_range(f, 55.0, 65.0) => 0.0,
            f if in_range(f, 45.0, 75.0) => 2.0,
            _ => 1.0,
        });
        let got = window_imbalance(&grid, &psd_a, &psd_b, BAND);
        assert_eq!(got.state, ImbalanceState::NoneFound);
        assert_eq!(got.figure, 1.0);
    }

    #[test]
    fn a_one_zero_window_below_the_floor_is_skipped() {
        // A is silent in 55 to 65 Hz and B holds 0.001 per point there: a window sum of 0.02
        // (20 points) against a floor of about 1, so the window is skipped.
        let grid = grid_at(0.5);
        let psd_a = values(&grid, |f| if in_range(f, 55.0, 65.0) { 0.0 } else { 1.0 });
        let psd_b = values(&grid, |f| match f {
            f if in_range(f, 55.0, 65.0) => 0.001,
            f if in_range(f, 45.0, 75.0) => 2.0,
            _ => 1.0,
        });
        let got = window_imbalance(&grid, &psd_a, &psd_b, BAND);
        assert_eq!(got.state, ImbalanceState::NoneFound);
    }

    #[test]
    fn dominance_alternating_between_belts_has_no_run() {
        let grid = grid_at(0.5);
        let b_wins = |f: f64| (((f - 5.0) / 10.0).floor() as i64) % 2 == 0;
        let psd_a = values(&grid, |f| if b_wins(f) { 1.0 } else { 2.0 });
        let psd_b = values(&grid, |f| if b_wins(f) { 2.0 } else { 1.0 });
        let got = window_imbalance(&grid, &psd_a, &psd_b, BAND);
        assert_eq!(got.state, ImbalanceState::NoneFound);
        assert_eq!(got.figure, 1.0);
        assert_eq!(score(80.0, got.figure), "Excellent mechanical health");
    }

    #[test]
    fn fewer_than_three_eligible_windows_or_a_band_under_thirty_hz_is_unavailable() {
        // Silent but for two windows: the floor is 0 and only two windows are above it.
        let two = |f: f64| if in_range(f, 47.0, 63.0) { 1.0 } else { 0.0 };
        let few = compare_fn(two, |f| 0.5 * two(f), BAND);
        assert_unavailable_and_unscaled(&few);
        let wide = |f: f64| (-((f - 30.0) / 4.0).powi(2) / 2.0).exp();
        let narrow = compare_fn(wide, |f| 0.5 * wide(f), Some((20.0, 45.0)));
        assert_unavailable_and_unscaled(&narrow);
    }

    #[test]
    fn a_wide_feature_centred_on_a_window_edge_is_pinned_as_the_limitation() {
        // A broad bump on belt B alone, centred on 55 Hz, lifts the windows on both sides of
        // the edge and their neighbours enough to read as a run.
        let got = imbalance_of_b(|f| 1.0 + 5.0 * (-((f - 55.0) / 8.0).powi(2) / 2.0).exp());
        assert_eq!(got.state, ImbalanceState::Found);
        assert_eq!(got.stronger, Some(Belt::B));
        assert_eq!((got.lo_hz, got.hi_hz), (35.0, 65.0));
        assert!((got.figure - 0.2).abs() < 0.05, "{}", got.figure);
    }

    #[test]
    fn the_multiplier_is_one_at_and_above_0_8_figure_over_0_8_below_and_zero_at_zero() {
        // Similarity 68 with no pairs: an uncapped multiplier at 0.9 (1.125) would read 76.5,
        // Excellent. The rest give 68, 51 (0.6 / 0.8 = 0.75), 34 (0.4 / 0.8 = 0.5) and 0.
        assert_eq!(score(68.0, 0.9), "Good mechanical health");
        assert_eq!(score(68.0, 0.8), "Good mechanical health");
        assert_eq!(score(68.0, 0.6), "Acceptable mechanical health");
        assert_eq!(score(68.0, 0.4), "Potential signs of a mechanical issue");
        assert_eq!(score(68.0, 0.0), "Mechanical issue detected");
    }

    #[test]
    fn health_uses_the_tolerance_multiplier_end_to_end() {
        // Two curves with one peak at 20 Hz on a low floor: similarity near 100, one pair. B is
        // half of A over 65 to 95 Hz, where only the floor is left, so the figure is 0.5 and the
        // score about 62.5: Good. The raw figure would give 50, Acceptable; the square root
        // about 70.7, Excellent.
        let peak = |f: f64| (-((f - 20.0) / 8.0).powi(2) / 2.0).exp() + 0.05;
        let got = compare_fn(
            |f| peak(f) * if in_range(f, 65.0, 95.0) { 2.0 } else { 1.0 },
            peak,
            BAND,
        );
        assert_eq!(got.imbalance.state, ImbalanceState::Found);
        assert!((got.imbalance.figure - 0.5).abs() < 1e-9);
        assert!(got.pairs.len() <= 2);
        assert!(got.similarity > 90.0, "{}", got.similarity);
        let base = got.similarity * (got.imbalance.figure / TOLERANCE);
        assert!(base > 55.0 && base <= 70.0, "{base}");
        // The raw figure and the square root would read differently.
        assert!(got.similarity * got.imbalance.figure <= 55.0);
        assert!(
            got.similarity * got.imbalance.figure.sqrt() > 70.0,
            "{}",
            got.similarity
        );
        assert_eq!(got.health, "Good mechanical health");
    }

    #[test]
    fn the_lowest_of_tied_runs_wins() {
        let grid = grid_at(0.5);
        let psd_a = values(&grid, |f| if in_range(f, 65.0, 95.0) { 2.0 } else { 1.0 });
        let psd_b = values(&grid, |f| if in_range(f, 15.0, 45.0) { 2.0 } else { 1.0 });
        let got = window_imbalance(&grid, &psd_a, &psd_b, BAND);
        assert_eq!(got.state, ImbalanceState::Found);
        assert_eq!((got.lo_hz, got.hi_hz), (15.0, 45.0));
        assert_eq!(got.stronger, Some(Belt::B));
    }

    #[test]
    fn swapping_the_belts_keeps_the_figure_and_flips_the_direction() {
        let grid = grid_at(0.5);
        let flat = vec![1.0; grid.len()];
        let high = values(&grid, twice_over(45.0, 75.0));
        let forward = window_imbalance(&grid, &flat, &high, BAND);
        let swapped = window_imbalance(&grid, &high, &flat, BAND);
        assert_eq!(forward.figure, swapped.figure);
        assert_eq!(
            (forward.lo_hz, forward.hi_hz),
            (swapped.lo_hz, swapped.hi_hz)
        );
        assert_eq!(forward.stronger, Some(Belt::B));
        assert_eq!(swapped.stronger, Some(Belt::A));
    }

    #[test]
    fn a_sweep_above_max_freq_makes_no_window_past_the_grid() {
        let grid = grid_at(0.5);
        let flat = vec![1.0; grid.len()];
        // B is 2x from 85 Hz on; only the 85 to 95 window is whole inside the grid.
        let high = values(&grid, |f| if f >= 85.0 { 2.0 } else { 1.0 });
        let past = window_imbalance(&grid, &flat, &high, Some((5.0, 500.0)));
        assert_eq!(past, window_imbalance(&grid, &flat, &high, BAND));
        assert_eq!(past.state, ImbalanceState::NoneFound);
        let wide = window_imbalance(
            &grid,
            &flat,
            &values(&grid, twice_over(65.0, 95.0)),
            Some((5.0, 500.0)),
        );
        assert!(wide.hi_hz <= 100.0, "{}", wide.hi_hz);
    }

    #[test]
    fn every_health_band_boundary_is_crossed_by_a_known_figure() {
        // At similarity 100 with no pairs the score is 100 times figure / TOLERANCE.
        let cases = [
            (
                0.70,
                "Excellent mechanical health",
                "Good mechanical health",
            ),
            (
                0.55,
                "Good mechanical health",
                "Acceptable mechanical health",
            ),
            (
                0.45,
                "Acceptable mechanical health",
                "Potential signs of a mechanical issue",
            ),
            (
                0.30,
                "Potential signs of a mechanical issue",
                "Likely a mechanical issue",
            ),
            (
                0.15,
                "Likely a mechanical issue",
                "Mechanical issue detected",
            ),
        ];
        for (boundary, above, below) in cases {
            assert_eq!(
                score(100.0, (boundary + 0.005) * TOLERANCE),
                above,
                "figure above {boundary}"
            );
            assert_eq!(
                score(100.0, (boundary - 0.005) * TOLERANCE),
                below,
                "figure below {boundary}"
            );
        }
        assert_eq!(score(100.0, 0.0), "Mechanical issue detected");
    }

    #[test]
    fn flat_and_all_zero_curves_keep_their_score_and_never_give_a_nan_figure() {
        let flat = compare_fn(|_| 1.0, |_| 1.0, None);
        assert_eq!(flat.imbalance.state, ImbalanceState::NoneFound);
        assert_eq!(flat.imbalance.figure, 1.0);
        assert_eq!(flat.health, "Mechanical issue detected");
        let zero = compare_fn(|_| 0.0, |_| 0.0, None);
        assert_eq!(zero.imbalance.state, ImbalanceState::Unavailable);
        assert_eq!(zero.imbalance.figure, 1.0);
        assert_eq!(zero.health, "Mechanical issue detected");
        let one_flat = compare_fn(|_| 1.0, |f| (-((f - 50.0) / 4.0).powi(2) / 2.0).exp(), None);
        assert!(!one_flat.imbalance.figure.is_nan());
    }
}
