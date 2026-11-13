//! Comparison of two belts' PSDs, computed the way Shake&Tune's belt comparison computes it
//! (`belts_computation.py` and the offset table of `belts_plotter.py`, at the commit pinned in
//! `docs/specs/shaketune-deviations.md`).
//!
//! The port keeps the defects D8 to D17 of that file on purpose, because it reproduces the
//! figures Shake&Tune prints. A test covers each one and none is fixed here. They include
//! duplicate indices in the peak lists, which come from the peak detector's wrap of negative
//! indices.

use serde::{Deserialize, Serialize};

use crate::numeric::interp;
use crate::peaks::detect_peaks_with;

pub const GRID_POINTS: usize = 500;
/// A peak must rise above this fraction of its own curve's maximum.
pub const PEAK_THRESHOLD: f64 = 0.1;
/// Width of the moving average, in grid points, not Hz.
pub const PEAK_WINDOW: usize = 20;
/// Reach of the peak refinement, in grid points, not Hz.
pub const PEAK_VICINITY: usize = 15;
/// Cap on the pairing cutoff in Hz. The comparison is `distance < cutoff + 1`, so the real
/// limit is one Hz more.
pub const PAIR_CAP_HZ: f64 = 10.0;
/// Number of paired peaks above which the health score is scaled down.
pub const MAX_PEAKS: usize = 2;
/// Health points taken off for an unpaired peak as tall as the highest value of either curve.
pub const UNPAIRED_PENALTY: f64 = 30.0;

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
    /// Difference of belt A's curve at the two peaks, as a percentage of the highest value of
    /// either curve. Always 0 when both peaks share a bin (D8).
    pub amplitude_delta_percent: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BeltsResult {
    /// The common grid in Hz, `GRID_POINTS` values from 0 to `max_freq`.
    pub freqs: Vec<f64>,
    pub a: BeltCurve,
    pub b: BeltCurve,
    /// Peaks matched across the belts, in the order the greedy match found them (D16).
    pub pairs: Vec<BeltPair>,
    pub offsets: Vec<PeakOffset>,
    /// Pearson correlation of the two curves times 100, clipped to 0..100. NaN when a curve is
    /// flat (D11), as in Shake&Tune. serde_json writes NaN as `null`, which it cannot read back
    /// into an `f64`.
    pub similarity: f64,
    /// Shake&Tune's mechanical health text.
    pub health: String,
    pub max_freq: f64,
}

/// Compares two PSDs. Each is cut at `max_freq` and resampled onto `GRID_POINTS` points from 0
/// to `max_freq`, and the peaks, pairs, similarity and health are worked out on that grid.
pub fn compare_psds(
    freqs_a: &[f64],
    psd_a: &[f64],
    freqs_b: &[f64],
    psd_b: &[f64],
    max_freq: f64,
) -> BeltsResult {
    let grid = linspace(0.0, max_freq, GRID_POINTS);
    let (psd_a, peaks_a) = resample_and_find_peaks(&grid, freqs_a, psd_a, max_freq);
    let (psd_b, peaks_b) = resample_and_find_peaks(&grid, freqs_b, psd_b, max_freq);

    let (index_pairs, unpaired_a, unpaired_b) = pair_peaks(&peaks_a, &peaks_b, &grid);
    let highest = nan_max(nan_max(f64::NEG_INFINITY, &psd_a), &psd_b);

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
            amplitude_delta_percent: ((psd_a[j] - psd_a[i]) / highest).abs() * 100.0,
        })
        .collect();

    let similarity = (correlation(&psd_a, &psd_b) * 100.0).clamp(0.0, 100.0);
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
    let health = mechanical_health(similarity, &a, &b, pairs.len(), highest);
    BeltsResult {
        freqs: grid,
        a,
        b,
        pairs,
        offsets,
        similarity,
        health,
        max_freq,
    }
}

fn resample_and_find_peaks(
    grid: &[f64],
    freqs: &[f64],
    psd: &[f64],
    max_freq: f64,
) -> (Vec<f64>, Vec<usize>) {
    let (cut_freqs, cut_psd): (Vec<f64>, Vec<f64>) = freqs
        .iter()
        .zip(psd)
        .filter(|(f, _)| **f <= max_freq)
        .map(|(f, p)| (*f, *p))
        .unzip();
    let resampled = interp(grid, &cut_freqs, &cut_psd);
    let threshold = PEAK_THRESHOLD * nan_max(f64::NEG_INFINITY, &resampled);
    let peaks = detect_peaks_with(&resampled, threshold, PEAK_WINDOW, PEAK_VICINITY);
    (resampled, peaks)
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
        let mut min_distance = threshold + 1.0;
        let mut best = None;
        for &p in &left_a {
            for &q in &left_b {
                let distance = (grid[p] - grid[q]).abs();
                if distance < min_distance {
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

/// `_compute_mhi` and `_mhi_lut`. A NaN similarity stays NaN through the score and reads as
/// unknown.
fn mechanical_health(
    similarity: f64,
    a: &BeltCurve,
    b: &BeltCurve,
    pair_count: usize,
    highest: f64,
) -> String {
    let mut score = similarity;
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

/// Bands are open at the bottom, so exactly 0 matches none (D12).
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
    } else if score > 0.0 {
        "Mechanical issue detected"
    } else {
        "Unknown mechanical health"
    };
    text.to_owned()
}

/// `numpy.linspace` with the endpoint included.
fn linspace(start: f64, stop: f64, n: usize) -> Vec<f64> {
    let step = (stop - start) / (n - 1) as f64;
    let mut out: Vec<f64> = (0..n).map(|i| i as f64 * step + start).collect();
    out[n - 1] = stop;
    out
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

/// `numpy.median` of sorted values.
fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// `numpy.percentile` of sorted values, linear interpolation.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    let position = (sorted.len() - 1) as f64 * (q / 100.0);
    let below = position.floor();
    let t = position - below;
    let lo = sorted[below as usize];
    let hi = sorted[(below as usize + 1).min(sorted.len() - 1)];
    // numpy's `_lerp`, which counts from the nearer end.
    if t >= 0.5 {
        hi - (hi - lo) * (1.0 - t)
    } else {
        lo + (hi - lo) * t
    }
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

    /// A PSD on 0..=200 Hz at 0.25 Hz with a Gaussian of 2 Hz width at each `(freq, height)`.
    fn modes(modes: &[(f64, f64)]) -> (Vec<f64>, Vec<f64>) {
        let freqs: Vec<f64> = (0..=800).map(|i| i as f64 * 0.25).collect();
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
        assert_eq!(got.health, "Potential signs of a mechanical issue");
    }

    #[test]
    fn the_amplitude_delta_of_peaks_in_one_bin_is_zero() {
        let got = compare(&[(50.0, 1.0)], &[(50.0, 5.0)], MAX_FREQ);
        assert_eq!(got.offsets.len(), 1);
        assert_eq!(got.offsets[0].amplitude_delta_percent, 0.0);
    }

    #[test]
    fn the_offsets_read_belt_a_at_both_peaks_of_a_pair() {
        // Belt A has a tall peak at 50 Hz and a short one at 56 Hz, which belt B merges into
        // one peak at 56 Hz. The pair's offsets come from belt A's curve at 50 and 56 Hz.
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
        // belts_plotter.py `_add_offset_table`: abs((psd[peak2] - psd[peak1]) / highest * 100)
        // with psd and freqs taken from belt A (signal1).
        let want_amplitude = ((got.a.psd[j] - got.a.psd[i]) / highest * 100.0).abs();
        let want_freq = (got.freqs[j] - got.freqs[i]).abs();
        assert!(want_amplitude > 1.0);
        assert!((got.offsets[0].freq_delta - want_freq).abs() < 1e-12);
        assert!((got.offsets[0].amplitude_delta_percent - want_amplitude).abs() < 1e-9);
        assert!(got.offsets[0].freq_delta > 0.0);
        // Belt B's own curve would give another number, so this reads belt A.
        let from_b = ((got.b.psd[j] - got.b.psd[i]) / highest * 100.0).abs();
        assert!((got.offsets[0].amplitude_delta_percent - from_b).abs() > 1.0);
    }

    #[test]
    fn the_pairing_cutoff_is_eleven_hz_not_ten() {
        let near = compare(&[(40.0, 1.0)], &[(50.5, 1.0)], MAX_FREQ);
        assert_eq!(near.pairs.len(), 1);
        let far = compare(&[(40.0, 1.0)], &[(51.5, 1.0)], MAX_FREQ);
        assert!(far.pairs.is_empty());
    }

    #[test]
    fn a_flat_curve_gives_nan_and_unknown_health() {
        let freqs: Vec<f64> = (0..=400).map(|i| i as f64 * 0.25).collect();
        let flat = vec![1.0; freqs.len()];
        let (fb, pb) = modes(&[(50.0, 1.0)]);
        let got = compare_psds(&freqs, &flat, &fb, &pb, MAX_FREQ);
        assert!(got.similarity.is_nan());
        assert_eq!(got.health, "Unknown mechanical health");
    }

    #[test]
    fn an_anticorrelated_pair_reads_unknown_not_issue() {
        let got = compare(&[(30.0, 1.0)], &[(80.0, 1.0)], MAX_FREQ);
        assert_eq!(got.similarity, 0.0);
        assert_eq!(got.health, "Unknown mechanical health");
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
    fn the_peak_count_depends_on_max_freq() {
        let both = [(50.0, 1.0), (57.0, 1.0)];
        assert_eq!(compare(&both, &both, 100.0).a.peaks.len(), 2);
        assert_eq!(compare(&both, &both, 200.0).a.peaks.len(), 1);
    }

    #[test]
    fn every_health_band_is_open_at_the_bottom_and_closed_at_the_top() {
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
            (0.0, "Unknown mechanical health"),
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
            mechanical_health(80.0, &a, &b, 2, 1.0),
            "Excellent mechanical health"
        );
        assert_eq!(
            mechanical_health(80.0, &a, &b, 4, 1.0),
            "Potential signs of a mechanical issue"
        );
    }

    #[test]
    fn health_takes_30_points_for_an_unpaired_peak_as_tall_as_the_highest() {
        let a = curve(&[3], &[3]);
        let b = curve(&[], &[]);
        assert_eq!(
            mechanical_health(100.0, &a, &b, 0, 1.0),
            "Good mechanical health"
        );
        assert_eq!(
            mechanical_health(80.0, &a, &b, 0, 1.0),
            "Acceptable mechanical health"
        );
        let both = curve(&[3], &[3]);
        assert_eq!(
            mechanical_health(100.0, &a, &both, 0, 1.0),
            "Potential signs of a mechanical issue"
        );
        assert_eq!(
            mechanical_health(50.0, &a, &both, 0, 1.0),
            "Unknown mechanical health"
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
    fn a_nan_similarity_serialises_as_null() {
        let (f, p) = modes(&[(50.0, 1.0)]);
        let flat = vec![1.0; f.len()];
        let got = compare_psds(&f, &flat, &f, &p, MAX_FREQ);
        let json = serde_json::to_value(&got).expect("serialises");
        assert!(json["similarity"].is_null());
    }
}
