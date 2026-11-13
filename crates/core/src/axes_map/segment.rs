//! Per-segment motion detection: a port of Shake&Tune's `_process_single_axis`.

use super::SegmentCurves;
use crate::error::CoreError;
use crate::numeric::strictly_increasing;
use crate::samples::RawSamples;

/// Fewest samples a segment may have.
const MIN_SAMPLES: usize = 10;
/// Sample rate assumed when the timestamps span no time.
const FALLBACK_RATE: f64 = 3200.0;
/// Cutoff (Hz) the low-pass boxcar width is derived from.
const FILTER_CUTOFF: f64 = 25.0;
/// Most points kept per plotted velocity curve.
const MAX_CURVE_POINTS: usize = 2000;

/// What one sweep segment revealed about the chip orientation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SegmentResult {
    /// Unit vector (in chip axes) of the dominant axis, signed by its peak.
    pub direction_vector: [f64; 3],
    /// Normalised vector of the three signed peak velocities.
    pub actual_direction: [f64; 3],
    /// Angle (degrees) between `actual_direction` and `direction_vector`.
    pub angle_error: f64,
    /// Dominance of the strongest axis over the second strongest, in [0, 1].
    pub confidence: f64,
    /// Mean MAD noise estimate over the three axes.
    pub noise_level: f64,
    /// Signed peak velocity per chip axis.
    pub peak_velocities: [f64; 3],
    /// Magnitude of the per-axis median acceleration.
    pub gravity: f64,
    /// Time of the matched axis's velocity peak since the first sample (s).
    pub peak_time_s: f64,
    /// Decimated corrected velocities for plotting.
    pub curves: SegmentCurves,
}

/// Detect which chip axis carries the motion in one segment.
///
/// `inverse` undoes a configured `axes_map` before analysis.
///
/// # Errors
/// `MismatchedColumns`, `NonFiniteSample`, `InvalidTimestamps` or `TooFewSamples`.
pub(crate) fn process_segment(
    samples: &RawSamples,
    inverse: Option<&[[f64; 3]; 3]>,
) -> Result<SegmentResult, CoreError> {
    let n = samples.t.len();
    if samples.x.len() != n || samples.y.len() != n || samples.z.len() != n {
        return Err(CoreError::MismatchedColumns);
    }
    if [&samples.x, &samples.y, &samples.z]
        .iter()
        .any(|c| c.iter().any(|v| !v.is_finite()))
    {
        return Err(CoreError::NonFiniteSample);
    }
    if !strictly_increasing(&samples.t) {
        return Err(CoreError::InvalidTimestamps);
    }
    if n < MIN_SAMPLES {
        return Err(CoreError::TooFewSamples {
            have: n,
            need: MIN_SAMPLES,
        });
    }

    let time = &samples.t;
    let mut accel = [samples.x.clone(), samples.y.clone(), samples.z.clone()];
    if let Some(m) = inverse {
        let mut out = [vec![0.0; n], vec![0.0; n], vec![0.0; n]];
        for (row, o) in m.iter().zip(out.iter_mut()) {
            for (i, v) in o.iter_mut().enumerate() {
                *v = row[0] * accel[0][i] + row[1] * accel[1][i] + row[2] * accel[2][i];
            }
        }
        accel = out;
    }

    let rate = if time[n - 1] > time[0] {
        n as f64 / (time[n - 1] - time[0])
    } else {
        FALLBACK_RATE
    };

    // Remove gravity (per-axis median).
    let medians = [median(&accel[0]), median(&accel[1]), median(&accel[2])];
    let gravity = medians.iter().map(|g| g * g).sum::<f64>().sqrt();
    for (a, g) in accel.iter_mut().zip(medians) {
        a.iter_mut().for_each(|v| *v -= g);
    }

    let noise_level = accel.iter().map(|a| noise_level(a, rate)).sum::<f64>() / 3.0;

    let velocity: [Vec<f64>; 3] = std::array::from_fn(|k| {
        let filtered = lowpass(&accel[k], rate);
        correct_drift(integrate(&filtered, time))
    });

    let mut peaks = [0.0; 3];
    for (p, v) in peaks.iter_mut().zip(&velocity) {
        let max = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let min = v.iter().copied().fold(f64::INFINITY, f64::min);
        *p = if max.abs() >= min.abs() { max } else { min };
    }

    let norm = peaks.iter().map(|p| p * p).sum::<f64>().sqrt();
    let actual_direction = if norm > 0.0 {
        peaks.map(|p| p / norm)
    } else {
        peaks
    };

    // First axis wins ties, like Python's `max` over an ordered dict.
    let mut primary = 0;
    for k in 1..3 {
        if peaks[k].abs() > peaks[primary].abs() {
            primary = k;
        }
    }
    let sign = if peaks[primary] > 0.0 { 1.0 } else { -1.0 };
    let mut direction_vector = [0.0; 3];
    direction_vector[primary] = sign;

    let dot = actual_direction[primary] * sign;
    let angle_error = dot.clamp(-1.0, 1.0).acos().to_degrees();

    let mut sorted: Vec<f64> = peaks.iter().map(|p| p.abs()).collect();
    sorted.sort_by(|a, b| b.total_cmp(a));
    let dominance = if sorted[1] > 0.0 {
        sorted[0] / sorted[1]
    } else {
        f64::INFINITY
    };
    let confidence = ((dominance - 1.0) / 4.0).clamp(0.0, 1.0);

    // First occurrence of the extreme, like numpy's argmax/argmin.
    let matched = &velocity[primary];
    let mut peak_index = 0;
    for (i, v) in matched.iter().enumerate() {
        let better = if peaks[primary] > 0.0 {
            *v > matched[peak_index]
        } else {
            *v < matched[peak_index]
        };
        if better {
            peak_index = i;
        }
    }
    let peak_time_s = time[peak_index] - time[0];

    let step = n.div_ceil(MAX_CURVE_POINTS);
    let curves = SegmentCurves {
        t: time.iter().step_by(step).map(|t| t - time[0]).collect(),
        velocity: std::array::from_fn(|k| velocity[k].iter().step_by(step).copied().collect()),
    };

    Ok(SegmentResult {
        direction_vector,
        actual_direction,
        angle_error,
        confidence,
        noise_level,
        peak_velocities: peaks,
        gravity,
        peak_time_s,
        curves,
    })
}

/// numpy's `median`: the mean of the two middle values for an even count.
fn median(data: &[f64]) -> f64 {
    let mut sorted = data.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// `np.convolve(data, ones(width) / width, mode='same')` for odd `width`: a centred,
/// zero-padded moving average.
fn boxcar(data: &[f64], width: usize) -> Vec<f64> {
    let half = width / 2;
    let weight = 1.0 / width as f64;
    let n = data.len();
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(n);
            data[lo..hi].iter().map(|v| v * weight).sum()
        })
        .collect()
}

/// Two boxcar passes; the width approximates a 25 Hz cutoff.
fn lowpass(data: &[f64], rate: f64) -> Vec<f64> {
    let width = ((rate / FILTER_CUTOFF / 2.0) as usize | 1).max(3);
    boxcar(&boxcar(data, width), width)
}

/// Trapezoid-rule integral of `accel` over `time`, starting at 0.
fn integrate(accel: &[f64], time: &[f64]) -> Vec<f64> {
    let mut velocity = vec![0.0; accel.len()];
    let mut sum = 0.0;
    for i in 1..accel.len() {
        sum += (accel[i - 1] + accel[i]) / 2.0 * (time[i] - time[i - 1]);
        velocity[i] = sum;
    }
    velocity
}

/// Subtract the straight line from the first to the last sample.
fn correct_drift(mut velocity: Vec<f64>) -> Vec<f64> {
    let n = velocity.len();
    if n < 2 {
        return velocity;
    }
    let first = velocity[0];
    let slope = (velocity[n - 1] - first) / (n - 1) as f64;
    for (i, v) in velocity.iter_mut().enumerate() {
        *v -= first + slope * i as f64;
    }
    velocity
}

/// MAD of the residual against a ~10 ms moving average; 0 under 10 samples.
fn noise_level(accel: &[f64], rate: f64) -> f64 {
    if accel.len() < MIN_SAMPLES {
        return 0.0;
    }
    let mut width = ((rate * 0.01) as usize).max(5);
    if width.is_multiple_of(2) {
        width += 1;
    }
    let smoothed = boxcar(accel, width);
    let residual: Vec<f64> = accel.iter().zip(&smoothed).map(|(a, s)| a - s).collect();
    let centre = median(&residual);
    let deviations: Vec<f64> = residual.iter().map(|r| (r - centre).abs()).collect();
    1.4826 * median(&deviations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axes_map::golden;
    use crate::axes_map::map::inverse_matrix;

    const REL_TOL: f64 = 1e-9;
    const ABS_TOL: f64 = 1e-12;

    fn close(label: &str, got: f64, want: f64) {
        let tol = ABS_TOL + REL_TOL * want.abs();
        assert!(
            (got - want).abs() <= tol,
            "{label}: got {got:e}, want {want:e}, diff {:e} > tol {tol:e}",
            (got - want).abs()
        );
    }

    fn close_all(label: &str, got: &[f64; 3], want: &[f64; 3]) {
        for (k, (g, w)) in got.iter().zip(want).enumerate() {
            close(&format!("{label}[{k}]"), *g, *w);
        }
    }

    fn ramp(n: usize) -> RawSamples {
        RawSamples {
            t: (0..n).map(|i| i as f64 / 1000.0).collect(),
            x: (0..n).map(|i| (i as f64 * 0.3).sin()).collect(),
            y: vec![0.0; n],
            z: vec![9810.0; n],
        }
    }

    #[test]
    fn per_segment_values_match_the_reference() {
        let golden = golden::load();
        for (name, case) in &golden.cases {
            let inverse = inverse_matrix(case.configured_map.as_deref());
            assert_eq!(case.segments.len(), case.expected.per_segment.len());
            for (i, (segment, want)) in case
                .segments
                .iter()
                .zip(&case.expected.per_segment)
                .enumerate()
            {
                let got = process_segment(&segment.samples(), inverse.as_ref()).unwrap();
                let l = |what: &str| format!("{name} segment {i} {what}");
                close_all(
                    &l("direction_vector"),
                    &got.direction_vector,
                    &want.direction_vector,
                );
                close_all(
                    &l("actual_direction"),
                    &got.actual_direction,
                    &want.actual_direction,
                );
                close(&l("angle_error"), got.angle_error, want.angle_error);
                close(&l("confidence"), got.confidence, want.confidence);
                close(&l("noise_level"), got.noise_level, want.noise_level);
                close_all(
                    &l("peak_velocities"),
                    &got.peak_velocities,
                    &want.peak_velocities,
                );
                close(&l("gravity"), got.gravity, want.gravity);
            }
        }
    }

    #[test]
    fn short_recordings_are_refused() {
        assert_eq!(
            process_segment(&ramp(9), None).unwrap_err(),
            CoreError::TooFewSamples { have: 9, need: 10 }
        );
    }

    #[test]
    fn non_finite_and_unsorted_input_is_refused() {
        let mut s = ramp(50);
        s.x[3] = f64::NAN;
        assert_eq!(
            process_segment(&s, None).unwrap_err(),
            CoreError::NonFiniteSample
        );

        let mut s = ramp(50);
        s.t[10] = s.t[9];
        assert_eq!(
            process_segment(&s, None).unwrap_err(),
            CoreError::InvalidTimestamps
        );

        let mut s = ramp(50);
        s.y.pop();
        assert_eq!(
            process_segment(&s, None).unwrap_err(),
            CoreError::MismatchedColumns
        );
    }

    /// The reference gives confidence 1.0 here (second-largest peak is 0, so the dominance
    /// ratio is infinite) and picks X with a negative sign because 0 is not above 0.
    #[test]
    fn flat_data_gives_zero_peaks_and_no_panic() {
        let s = RawSamples {
            t: (0..100).map(|i| i as f64 / 1000.0).collect(),
            x: vec![1.0; 100],
            y: vec![2.0; 100],
            z: vec![9810.0; 100],
        };
        let got = process_segment(&s, None).unwrap();
        assert_eq!(got.peak_velocities, [0.0; 3]);
        assert_eq!(got.confidence, 1.0);
        assert_eq!(got.direction_vector, [-1.0, 0.0, 0.0]);
    }
}
