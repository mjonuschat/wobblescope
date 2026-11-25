//! Per-segment motion detection: a port of Shake&Tune's `_process_single_axis`.

use super::SegmentCurves;
use crate::error::CoreError;
use crate::numeric::{median_sorted, strictly_increasing};
use crate::samples::RawSamples;

/// Fewest samples a segment may have.
const MIN_SAMPLES: usize = 10;
/// Sample rate assumed when the timestamps span no time.
const FALLBACK_RATE: f64 = 3200.0;
/// Cutoff (Hz) the low-pass boxcar width is derived from.
const FILTER_CUTOFF: f64 = 25.0;
/// Most points kept per plotted velocity curve.
const MAX_CURVE_POINTS: usize = 2000;
/// Width (s) of the window over which the motion's RMS is measured to find where the move is.
const ACTIVITY_WINDOW_S: f64 = 0.05;
/// Length (s) of the start of a segment taken as quiet when judging what counts as motion.
const QUIET_LEAD_S: f64 = 0.3;
/// Motion is where the RMS exceeds this multiple of the quiet RMS, plus the floor below.
const ACTIVITY_FACTOR: f64 = 2.0;
/// Acceleration (mm/s²) added to the motion threshold, so a very quiet lead-in does not make
/// every ripple count as motion.
const ACTIVITY_FLOOR: f64 = 300.0;
/// Seconds before the first motion that stay out of the rest level, and after the last.
const REST_GAP_BEFORE_S: f64 = 0.1;
const REST_GAP_AFTER_S: f64 = 0.2;
/// Shortest stretch (s) of rest before the move that gives a rest level.
const MIN_REST_S: f64 = 0.2;
/// Seconds either side of the move that are integrated.
const MOVE_MARGIN_S: f64 = 0.15;

/// Which of the two ways of reading a segment is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Method {
    /// Shake&Tune's: subtract the segment's median, integrate all of it and remove a straight
    /// line of drift. Only the tests that pin its numbers ask for it.
    Reference,
    /// Subtract the mean of the rest before the move up to the move and the mean of the rest
    /// after it from there on, integrate around the move and remove a straight line of drift.
    /// Reads like `Reference` when the segment has no rest to measure before the move.
    Wobblescope,
}

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
    /// The per-axis median acceleration in chip axes: gravity, up to the motion.
    pub gravity_vector: [f64; 3],
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
    method: Method,
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

    let stepped = match method {
        Method::Reference => None,
        Method::Wobblescope => stepped_velocity(&accel, time, rate),
    };
    let velocity: [Vec<f64>; 3] = stepped.unwrap_or_else(|| {
        std::array::from_fn(|k| {
            let filtered = lowpass(&accel[k], rate);
            correct_drift(integrate(&filtered, time))
        })
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
        gravity_vector: medians,
        peak_time_s,
        curves,
    })
}

/// `numpy.median`: the mean of the two middle values for an even count.
fn median(data: &[f64]) -> f64 {
    let mut sorted = data.to_vec();
    sorted.sort_by(f64::total_cmp);
    median_sorted(&sorted)
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

/// The RMS of the three axes together over a centred window of `width` samples.
fn joint_rms(accel: &[Vec<f64>; 3], width: usize) -> Vec<f64> {
    let n = accel[0].len();
    let half = width / 2;
    let prefix = |value: fn(f64) -> f64| -> Vec<Vec<f64>> {
        accel
            .iter()
            .map(|a| {
                let mut sums = Vec::with_capacity(n + 1);
                sums.push(0.0);
                let mut total = 0.0;
                for v in a {
                    total += value(*v);
                    sums.push(total);
                }
                sums
            })
            .collect()
    };
    let prefix_sums = prefix(|v| v);
    let prefix_squares = prefix(|v| v * v);
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(n);
            let count = (hi - lo) as f64;
            (0..3)
                .map(|k| {
                    let mean = (prefix_sums[k][hi] - prefix_sums[k][lo]) / count;
                    ((prefix_squares[k][hi] - prefix_squares[k][lo]) / count - mean * mean).max(0.0)
                })
                .sum::<f64>()
                .sqrt()
        })
        .collect()
}

/// The first and last sample of the move, found where the motion's RMS rises above that of the
/// quiet start of the segment. `None` when nothing rises above it.
fn move_span(accel: &[Vec<f64>; 3], rate: f64) -> Option<(usize, usize)> {
    let n = accel[0].len();
    let width = ((rate * ACTIVITY_WINDOW_S) as usize | 1).max(3);
    let rms = joint_rms(accel, width);
    let quiet_end = ((rate * QUIET_LEAD_S) as usize).min(n);
    // The first and last `width` samples have a window that does not fit.
    let quiet = if quiet_end > width + 20 {
        &rms[width..quiet_end]
    } else {
        &rms[..]
    };
    let mut sorted = quiet.to_vec();
    sorted.sort_by(f64::total_cmp);
    let threshold = ACTIVITY_FACTOR * median_sorted(&sorted) + ACTIVITY_FLOOR;
    let inside = width..n.saturating_sub(width);
    let mut moving = inside.filter(|&i| rms[i] > threshold);
    let first = moving.next()?;
    let last = moving.next_back().unwrap_or(first);
    Some((first, last))
}

fn mean(data: &[f64]) -> f64 {
    data.iter().sum::<f64>() / data.len() as f64
}

/// The velocities of the three axes with the baseline stepped across the move, or `None` when the
/// segment has no move to find or no rest before it. See [`Method::Wobblescope`].
///
/// The accelerometer's reading of gravity is not the same before and after a move: the toolhead
/// settles at a slightly different tilt, a fraction of a degree, which on a 1 g reading is tens of
/// mm/s². A single baseline cannot remove a step, and integrated it becomes a velocity of the
/// size of the move itself. The velocity outside the integrated stretch is zero, as the toolhead
/// is at rest there.
fn stepped_velocity(accel: &[Vec<f64>; 3], time: &[f64], rate: f64) -> Option<[Vec<f64>; 3]> {
    let n = time.len();
    let (first, last) = move_span(accel, rate)?;
    let at = |index: usize| time[index] - time[0];
    let before = |gap: f64| time.partition_point(|t| t - time[0] < at(first) - gap);
    let after = |gap: f64| time.partition_point(|t| t - time[0] <= at(last) + gap);
    let rest_before = before(REST_GAP_BEFORE_S);
    if (rest_before as f64) < MIN_REST_S * rate {
        return None;
    }
    let rest_after = after(REST_GAP_AFTER_S);
    let has_after = n - rest_after >= (MIN_REST_S * rate) as usize;
    let lo = time.partition_point(|t| t - time[0] < at(first) - MOVE_MARGIN_S);
    let hi = time.partition_point(|t| t - time[0] <= at(last) + MOVE_MARGIN_S);
    let middle = (first + last) / 2;
    Some(std::array::from_fn(|k| {
        let pre = mean(&accel[k][..rest_before]);
        let post = if has_after {
            mean(&accel[k][rest_after..])
        } else {
            pre
        };
        let centred: Vec<f64> = accel[k]
            .iter()
            .enumerate()
            .map(|(i, a)| a - if i < middle { pre } else { post })
            .collect();
        let filtered = lowpass(&centred, rate);
        let mut velocity = vec![0.0; n];
        let region = correct_drift(integrate(&filtered[lo..hi], &time[lo..hi]));
        velocity[lo..hi].copy_from_slice(&region);
        velocity
    }))
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
                let got = process_segment(&segment.samples(), inverse.as_ref(), Method::Reference)
                    .unwrap();
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
            process_segment(&ramp(9), None, Method::Reference).unwrap_err(),
            CoreError::TooFewSamples { have: 9, need: 10 }
        );
    }

    #[test]
    fn non_finite_and_unsorted_input_is_refused() {
        let mut s = ramp(50);
        s.x[3] = f64::NAN;
        assert_eq!(
            process_segment(&s, None, Method::Reference).unwrap_err(),
            CoreError::NonFiniteSample
        );

        let mut s = ramp(50);
        s.t[10] = s.t[9];
        assert_eq!(
            process_segment(&s, None, Method::Reference).unwrap_err(),
            CoreError::InvalidTimestamps
        );

        let mut s = ramp(50);
        s.y.pop();
        assert_eq!(
            process_segment(&s, None, Method::Reference).unwrap_err(),
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
        let got = process_segment(&s, None, Method::Reference).unwrap();
        assert_eq!(got.peak_velocities, [0.0; 3]);
        assert_eq!(got.confidence, 1.0);
        assert_eq!(got.direction_vector, [-1.0, 0.0, 0.0]);
    }

    const RATE: f64 = 1600.0;

    /// A 1.7 s segment with gravity on z and a 30 ms ripple on every axis. A pulse on `axis` at
    /// `at` seconds accelerates for 50 ms and brakes for 50 ms, so velocity peaks at 80 mm/s.
    /// `offsets` is the baseline of each axis before and after the move: a step in how the chip
    /// sits.
    fn pulse(axis: usize, at: f64, offsets: [[f64; 2]; 3]) -> RawSamples {
        let n = (1.7 * RATE) as usize;
        let mut s = RawSamples::default();
        for i in 0..n {
            let t = i as f64 / RATE;
            let mut a = [0.0, 0.0, 9810.0];
            for (k, v) in a.iter_mut().enumerate() {
                *v += 20.0 * (t * 2.0 * std::f64::consts::PI * 33.0 + k as f64).sin();
                *v += offsets[k][usize::from(t > at + 0.1)];
            }
            if (at..at + 0.05).contains(&t) {
                a[axis] += 1600.0;
            } else if (at + 0.05..at + 0.1).contains(&t) {
                a[axis] -= 1600.0;
            }
            s.t.push(t);
            s.x.push(a[0]);
            s.y.push(a[1]);
            s.z.push(a[2]);
        }
        s
    }

    fn step() -> [[f64; 2]; 3] {
        [[0.0; 2], [0.0, 0.0], [-70.0, 70.0]]
    }

    #[test]
    fn a_step_in_the_baseline_does_not_hide_the_move() {
        // The move is on z; the chip's reading of that axis is 140 mm/s² higher after it.
        let s = pulse(2, 0.75, step());
        let got = process_segment(&s, None, Method::Wobblescope).unwrap();
        assert_eq!(got.direction_vector, [0.0, 0.0, 1.0]);
        let peak = got.peak_velocities[2];
        assert!((peak - 80.0).abs() < 8.0, "peak {peak}");
        assert!(got.confidence > 0.9, "confidence {}", got.confidence);
    }

    #[test]
    fn the_reference_reading_is_thrown_off_by_the_step() {
        let s = pulse(2, 0.75, step());
        let got = process_segment(&s, None, Method::Reference).unwrap();
        assert!(
            got.peak_velocities[2].abs() < 60.0,
            "{:?}",
            got.peak_velocities
        );
    }

    #[test]
    fn the_velocity_is_zero_where_the_toolhead_rests() {
        let s = pulse(1, 0.75, [[0.0; 2]; 3]);
        let got = process_segment(&s, None, Method::Wobblescope).unwrap();
        let velocity = &got.curves.velocity[1];
        assert!(velocity[0].abs() < 1e-9);
        assert!(velocity.last().unwrap().abs() < 1e-9);
        assert!(velocity.iter().any(|v| v.abs() > 60.0));
    }

    #[test]
    fn a_segment_that_moves_at_once_reads_like_the_reference() {
        // The move starts 50 ms in, so there is no rest to take a baseline from.
        let s = pulse(1, 0.05, [[0.0; 2]; 3]);
        let a = process_segment(&s, None, Method::Wobblescope).unwrap();
        let b = process_segment(&s, None, Method::Reference).unwrap();
        assert_eq!(a.peak_velocities, b.peak_velocities);
        assert_eq!(a.confidence, b.confidence);
    }

    #[test]
    fn the_gravity_vector_is_the_median_of_each_axis() {
        let s = pulse(1, 0.75, [[0.0; 2]; 3]);
        let got = process_segment(&s, None, Method::Wobblescope).unwrap();
        assert!(got.gravity_vector[0].abs() < 50.0);
        assert!((got.gravity_vector[2] - 9810.0).abs() < 50.0);
    }
}
