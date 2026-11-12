//! Health checks on the sample values, which need no counters from the chip.

use crate::samples::RawSamples;

/// Standard gravity in mm/s², the unit of Klipper's accelerometer samples.
pub const GRAVITY: f64 = 9806.65;

/// The mean acceleration's magnitude must lie within this share of [`GRAVITY`], either way.
const GRAVITY_TOLERANCE: f64 = 0.25;

/// Fewer samples than this are not judged: the mean and the extremes mean nothing.
const MIN_SAMPLES: usize = 100;

/// An axis needs more distinct values than this before a repeated extreme counts as clipping.
/// A still sensor takes only a few values, and its extremes repeat without being clipped.
const MIN_DISTINCT_FOR_CLIPPING: usize = 20;

/// Clipping is only reported for an extreme at least this many times gravity in magnitude.
/// Chips clip at their full scale, and the smallest range in use is +-2 g; a recording that
/// rests at a smaller extreme is a quiet signal with a one-sided transient, not a clipped one.
const MIN_CLIP_LEVEL: f64 = 1.5 * GRAVITY;

/// At least this share of an axis's samples sitting exactly on its maximum or exactly on its
/// minimum counts as clipping.
const CLIPPED_SHARE: f64 = 0.01;

/// Problems in the sample values, one note each: an axis that never changes, an axis whose
/// extremes repeat as if clipped at the chip's full scale, or a mean acceleration far from
/// gravity, which points to wrong units or scaling. Empty for a recording under 100 samples.
#[must_use]
pub fn value_problems(samples: &RawSamples) -> Vec<String> {
    let n = samples.len();
    if n < MIN_SAMPLES {
        return Vec::new();
    }
    let mut notes = Vec::new();
    let mut means = [0.0; 3];
    for (k, (name, column)) in [("x", &samples.x), ("y", &samples.y), ("z", &samples.z)]
        .into_iter()
        .enumerate()
    {
        if column.len() != n || column.iter().any(|v| !v.is_finite()) {
            continue;
        }
        means[k] = column.iter().sum::<f64>() / n as f64;
        let (min, max) = column
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
                (lo.min(v), hi.max(v))
            });
        if min == max {
            notes.push(format!(
                "the {name} axis never changes from {min:.1}; the sensor may be disconnected"
            ));
            continue;
        }
        let mut distinct: Vec<f64> = column.clone();
        distinct.sort_by(f64::total_cmp);
        distinct.dedup();
        if distinct.len() > MIN_DISTINCT_FOR_CLIPPING {
            let at = |edge: f64| column.iter().filter(|&&v| v == edge).count();
            let low = if min.abs() >= MIN_CLIP_LEVEL {
                at(min)
            } else {
                0
            };
            let high = if max.abs() >= MIN_CLIP_LEVEL {
                at(max)
            } else {
                0
            };
            if low.max(high) as f64 >= CLIPPED_SHARE * n as f64 {
                notes.push(format!(
                    "the {name} axis sits at its extreme {} times out of {n}; the signal may be \
                     clipped at the chip's full scale",
                    low.max(high)
                ));
            }
        }
    }
    let magnitude = means.iter().map(|m| m * m).sum::<f64>().sqrt();
    if magnitude.is_finite() && ((magnitude - GRAVITY).abs() > GRAVITY * GRAVITY_TOLERANCE) {
        notes.push(format!(
            "the mean acceleration is {:.2} m/s², not near gravity (9.81 m/s²); the samples may \
             have wrong units or scaling",
            magnitude / 1000.0
        ));
    }
    notes
}
