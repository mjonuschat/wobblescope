//! Shaper response, remaining vibrations, smoothing and `max_accel`, ported from Klipper's
//! `shaper_calibrate.py`.

use std::f64::consts::PI;

use crate::error::CoreError;
use crate::shaper::{Impulses, SHAPER_VIBRATION_REDUCTION};

/// Amplitude of the vibration a shaper lets through at each frequency, relative to none.
///
/// An empty impulse sequence is the identity shaper: the response is 1 everywhere.
pub fn estimate_shaper(imp: &Impulses, damping_ratio: f64, freqs: &[f64]) -> Vec<f64> {
    if imp.a.is_empty() || imp.a.len() != imp.t.len() {
        return vec![1.0; freqs.len()];
    }
    let inv_d = 1.0 / imp.a.iter().sum::<f64>();
    let t_last = imp.t[imp.t.len() - 1];
    let df = (1.0 - damping_ratio * damping_ratio).sqrt();
    freqs
        .iter()
        .map(|&f| {
            let omega = 2.0 * PI * f;
            let damping = damping_ratio * omega;
            let omega_d = omega * df;
            let (mut s, mut c) = (0.0, 0.0);
            for (a, t) in imp.a.iter().zip(&imp.t) {
                let w = a * (-damping * (t_last - t)).exp();
                s += w * (omega_d * t).sin();
                c += w * (omega_d * t).cos();
            }
            (s * s + c * c).sqrt() * inv_d
        })
        .collect()
}

/// Fraction of the above-threshold vibration that remains after shaping, and the per-bin
/// response used to compute it.
///
/// # Errors
/// [`CoreError::FlatSignal`] when the PSD has no positive power: the ratio is undefined;
/// [`CoreError::InvalidParameter`] for empty or inconsistent impulses or arrays, a damping ratio
/// outside [0, 1), or PSD entries that are negative or not finite.
pub fn remaining_vibrations(
    imp: &Impulses,
    damping_ratio: f64,
    freq_bins: &[f64],
    psd: &[f64],
) -> Result<(f64, Vec<f64>), CoreError> {
    if imp.a.is_empty() || imp.a.len() != imp.t.len() || freq_bins.len() != psd.len() {
        return Err(CoreError::InvalidParameter(
            "impulses and frequency/PSD arrays must be non-empty and equal in length".to_owned(),
        ));
    }
    if !(damping_ratio.is_finite() && (0.0..1.0).contains(&damping_ratio)) {
        return Err(CoreError::InvalidParameter(format!(
            "damping ratio {damping_ratio} must be finite and in [0, 1)"
        )));
    }
    let vals = estimate_shaper(imp, damping_ratio, freq_bins);
    if psd.iter().any(|p| !p.is_finite() || *p < 0.0) {
        return Err(CoreError::InvalidParameter(
            "PSD entries must be finite and non-negative".to_owned(),
        ));
    }
    let max = psd.iter().copied().fold(0.0, f64::max);
    if max <= 0.0 {
        return Err(CoreError::FlatSignal);
    }
    // The ratio is scale-invariant; working on psd / max keeps the sums from overflowing.
    let threshold = 1.0 / SHAPER_VIBRATION_REDUCTION;
    let remaining: f64 = vals
        .iter()
        .zip(psd)
        .map(|(v, p)| (v * (p / max) - threshold).max(0.0))
        .sum();
    let all: f64 = psd.iter().map(|p| (p / max - threshold).max(0.0)).sum();
    let ratio = remaining / all;
    if !ratio.is_finite() || vals.iter().any(|v| !v.is_finite()) {
        return Err(CoreError::InvalidParameter(
            "non-finite shaper response or remaining-vibration ratio".to_owned(),
        ));
    }
    Ok((ratio, vals))
}

/// Estimated corner smoothing in mm for the shaper at `accel` with square corner velocity `scv`.
pub fn shaper_smoothing(imp: &Impulses, accel: f64, scv: f64) -> f64 {
    if imp.a.is_empty() || imp.a.len() != imp.t.len() {
        return 0.0;
    }
    let half_accel = accel * 0.5;
    let inv_d = 1.0 / imp.a.iter().sum::<f64>();
    let ts: f64 = imp.a.iter().zip(&imp.t).map(|(a, t)| a * t).sum::<f64>() * inv_d;
    let (mut offset_90, mut offset_180) = (0.0, 0.0);
    for (a, t) in imp.a.iter().zip(&imp.t) {
        if *t >= ts {
            offset_90 += a * (scv + half_accel * (t - ts)) * (t - ts);
        }
        offset_180 += a * half_accel * (t - ts).powi(2);
    }
    offset_90 *= inv_d * 2f64.sqrt();
    offset_180 *= inv_d;
    offset_90.max(offset_180)
}

fn bisect(func: impl Fn(f64) -> bool) -> f64 {
    let (mut left, mut right) = (1.0, 1.0);
    if !func(1e-9) {
        return 0.0;
    }
    while !func(left) {
        right = left;
        left *= 0.5;
        if left < 1e-9 {
            // `func` need not be monotonic; 1e-9 is known to satisfy it.
            left = 1e-9;
            break;
        }
    }
    if right == left {
        while func(right) && right < 1e15 {
            right *= 2.0;
        }
    }
    while right - left > 1e-8 {
        let middle = (left + right) * 0.5;
        if middle <= left || middle >= right {
            break;
        }
        if func(middle) {
            left = middle;
        } else {
            right = middle;
        }
    }
    left
}

/// Largest acceleration at which the shaper's smoothing stays at or below 0.12 mm.
///
/// This is a smoothing-based recommendation, not a machine capability limit: it knows nothing
/// about motors, frame or belts.
pub fn find_max_accel(imp: &Impulses, scv: f64) -> f64 {
    const TARGET_SMOOTHING: f64 = 0.12;
    bisect(|accel| shaper_smoothing(imp, accel, scv) <= TARGET_SMOOTHING)
}
