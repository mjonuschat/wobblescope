//! The damping ratio of the main resonance, from the width of its peak
//! (Shake&Tune's `compute_mechanical_parameters`, at the commit pinned in
//! `docs/specs/shaketune-deviations.md`).
//!
//! Shake&Tune measures the width where the PSD has fallen to the peak divided by the square root
//! of two, which is the half-power level of an amplitude, not of a power spectrum. The shaper
//! fit uses half the peak, and stops measuring where the spectrum turns up again towards a second
//! peak ([`estimate_damping`]); [`estimate_damping_above`], which the vibrations profile uses,
//! keeps Shake&Tune's level and walks on to the level.

use crate::shaper::DEFAULT_DAMPING_RATIO;

/// Fraction of the peak at which the bandwidth of a PSD is measured: half, since a PSD is power.
pub const HALF_POWER_LEVEL: f64 = 0.5;
/// The level Shake&Tune uses, where the shaper fit uses [`HALF_POWER_LEVEL`].
pub const SHAKETUNE_HALF_POWER_LEVEL: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// The damping ratio used when the data gives no estimate.
pub const FALLBACK_DAMPING: f64 = DEFAULT_DAMPING_RATIO;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DampingEstimate {
    /// Frequency of the strongest bin in Hz.
    pub fr: f64,
    /// The damping ratio, or `None` when the half-power points cannot be found or the formula
    /// has no real result.
    pub zeta: Option<f64>,
}

/// Shake&Tune's `compute_mechanical_parameters` for `psd` on `freq_bins`: the strongest bin
/// (the first when several tie), the two points on either side where the spectrum falls to
/// `HALF_POWER_LEVEL` times it (linear interpolation between bins), and the damping ratio from
/// the bandwidth between them. `None` for empty input or slices of different lengths.
pub fn estimate_damping(freq_bins: &[f64], psd: &[f64]) -> Option<DampingEstimate> {
    let n = freq_bins.len();
    if n == 0 || psd.len() != n {
        return None;
    }
    let peak = (0..n).fold(0, |best, i| if psd[i] > psd[best] { i } else { best });
    Some(half_power_estimate(
        freq_bins,
        psd,
        peak,
        HALF_POWER_LEVEL,
        true,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MechanicalParams {
    /// Frequency of the strongest bin at or above `min_freq`, in Hz.
    pub fr: f64,
    /// The damping ratio, `None` as for [`DampingEstimate::zeta`].
    pub zeta: Option<f64>,
    pub peak_index: usize,
    /// The strongest bin of the whole spectrum lies below `min_freq`.
    pub low_freq_max: bool,
}

/// Shake&Tune's `compute_mechanical_parameters` with a `min_freq`: the resonance is the strongest
/// bin at or above `min_freq`, the half-power points are searched over the whole spectrum as in
/// [`estimate_damping`], and `low_freq_max` tells when the strongest bin overall is below
/// `min_freq`. `None` when `min_freq` is above every bin, or for empty input or slices of
/// different lengths.
pub fn estimate_damping_above(
    freq_bins: &[f64],
    psd: &[f64],
    min_freq: f64,
) -> Option<MechanicalParams> {
    let n = freq_bins.len();
    if n == 0 || psd.len() != n {
        return None;
    }
    let first = freq_bins.partition_point(|f| *f < min_freq);
    if first >= n {
        return None;
    }
    let argmax =
        |from: usize| (from..n).fold(from, |best, i| if psd[i] > psd[best] { i } else { best });
    let low_freq_max = argmax(0) < first;
    let peak_index = argmax(first);
    let found = half_power_estimate(
        freq_bins,
        psd,
        peak_index,
        SHAKETUNE_HALF_POWER_LEVEL,
        false,
    );
    Some(MechanicalParams {
        fr: found.fr,
        zeta: found.zeta,
        peak_index,
        low_freq_max,
    })
}

/// How far the spectrum must climb out of a dip, as a fraction of the peak, for the dip to count
/// as the valley before a second peak and not as ripple on the peak's skirt.
const VALLEY_RISE: f64 = 0.1;

/// Whether the dip at `at` is a valley: walking on from it (through `ahead`, the indices in the
/// order walked) the spectrum climbs at least [`VALLEY_RISE`] of the peak before it falls to `half`.
fn is_valley(
    psd: &[f64],
    at: usize,
    ahead: impl Iterator<Item = usize>,
    half: f64,
    peak: f64,
) -> bool {
    let top = ahead
        .take_while(|&j| psd[j] > half)
        .map(|j| psd[j])
        .fold(psd[at], f64::max);
    top - psd[at] >= VALLEY_RISE * peak
}

#[derive(Clone, Copy)]
enum Edge {
    Half(usize),
    /// A local minimum still above the level, where the spectrum rises towards another peak.
    Valley(usize),
}

/// The edges of the peak at `peak`. With `stop_at_valleys`, a walk that meets a local minimum
/// above the level ends there, so a second peak close to the first is not measured as part of it.
fn edges(psd: &[f64], peak: usize, level: f64, stop_at_valleys: bool) -> Option<(Edge, Edge)> {
    let half = psd[peak] * level;
    if !stop_at_valleys {
        let below = psd[..peak].iter().rposition(|p| *p <= half)?;
        let above = psd[peak..].iter().position(|p| *p <= half)? + peak;
        return Some((Edge::Half(below), Edge::Half(above)));
    }
    let below = (0..peak).rev().find_map(|i| {
        if psd[i] <= half {
            Some(Edge::Half(i))
        } else if i > 0 && psd[i - 1] > psd[i] && is_valley(psd, i, (0..i).rev(), half, psd[peak]) {
            Some(Edge::Valley(i))
        } else {
            None
        }
    })?;
    let above = (peak + 1..psd.len()).find_map(|i| {
        if psd[i] <= half {
            Some(Edge::Half(i))
        } else if i + 1 < psd.len()
            && psd[i + 1] > psd[i]
            && is_valley(psd, i, i + 1..psd.len(), half, psd[peak])
        {
            Some(Edge::Valley(i))
        } else {
            None
        }
    })?;
    Some((below, above))
}

fn half_power_estimate(
    freq_bins: &[f64],
    psd: &[f64],
    peak: usize,
    level: f64,
    stop_at_valleys: bool,
) -> DampingEstimate {
    let fr = freq_bins[peak];
    let half = psd[peak] * level;
    let Some((lo, hi)) = edges(psd, peak, level, stop_at_valleys) else {
        return DampingEstimate { fr, zeta: None };
    };
    let f_below = match lo {
        Edge::Half(lo) => {
            freq_bins[lo]
                + (half - psd[lo]) * (freq_bins[lo + 1] - freq_bins[lo]) / (psd[lo + 1] - psd[lo])
        }
        Edge::Valley(lo) => freq_bins[lo],
    };
    let f_above = match hi {
        Edge::Half(hi) => {
            freq_bins[hi - 1]
                + (half - psd[hi - 1]) * (freq_bins[hi] - freq_bins[hi - 1])
                    / (psd[hi] - psd[hi - 1])
        }
        Edge::Valley(hi) => freq_bins[hi],
    };
    let ratio = (f_above - f_below) / fr;
    let (b1, b2) = (ratio.powi(2), ratio.powi(4));
    let inner = 1.0 / (4.0 + 4.0 * b1 - b2);
    let zeta = (inner.is_finite() && inner >= 0.0)
        .then(|| 0.5 - inner.sqrt())
        .filter(|x| x.is_finite() && *x >= 0.0)
        .map(f64::sqrt);
    DampingEstimate { fr, zeta }
}
