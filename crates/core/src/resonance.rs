//! The damping ratio of the main resonance, estimated the way Shake&Tune does
//! (`compute_mechanical_parameters`, at the commit pinned in `docs/specs/shaketune-deviations.md`).
//!
//! The estimate is kept exactly as Shake&Tune computes it, including where it is likely wrong
//! (it takes the half-power points at the peak divided by the square root of two); each
//! deliberate difference is a row in that file.

use crate::shaper::DEFAULT_DAMPING_RATIO;

/// Fraction of the peak at which the bandwidth is measured.
pub const HALF_POWER_LEVEL: f64 = std::f64::consts::FRAC_1_SQRT_2;
/// The damping ratio used when the data gives no estimate.
pub const FALLBACK_DAMPING: f64 = DEFAULT_DAMPING_RATIO;

/// The strongest resonance of a spectrum.
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
    let fr = freq_bins[peak];
    let half = psd[peak] * HALF_POWER_LEVEL;
    let below = psd[..peak].iter().rposition(|p| *p <= half);
    let above = psd[peak..]
        .iter()
        .position(|p| *p <= half)
        .map(|k| k + peak);
    let (Some(lo), Some(hi)) = (below, above) else {
        return Some(DampingEstimate { fr, zeta: None });
    };
    let f_below = freq_bins[lo]
        + (half - psd[lo]) * (freq_bins[lo + 1] - freq_bins[lo]) / (psd[lo + 1] - psd[lo]);
    let f_above = freq_bins[hi - 1]
        + (half - psd[hi - 1]) * (freq_bins[hi] - freq_bins[hi - 1]) / (psd[hi] - psd[hi - 1]);
    let ratio = (f_above - f_below) / fr;
    let (b1, b2) = (ratio.powi(2), ratio.powi(4));
    let inner = 1.0 / (4.0 + 4.0 * b1 - b2);
    let zeta = (inner.is_finite() && inner >= 0.0)
        .then(|| 0.5 - inner.sqrt())
        .filter(|x| x.is_finite() && *x >= 0.0)
        .map(f64::sqrt);
    Some(DampingEstimate { fr, zeta })
}
