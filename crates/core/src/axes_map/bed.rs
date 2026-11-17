//! Which segments of an axes-map run carry motion, and how the map is built from them.

use super::segment::SegmentResult;
use super::{AxesMapParams, Mounting};
use crate::error::CoreError;

/// Length of every sweep segment (mm).
const SEGMENT_LENGTH_MM: f64 = 30.0;
/// A segment is under the floor when its largest peak is below this share of its expected peak.
const FLOOR_FRACTION: f64 = 0.25;

/// What the three segments show, in terms of which printer axes can be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MotionCase {
    /// The chip saw no usable motion.
    NoMotion,
    /// All three axes are measured.
    Full,
    /// Two axes are measured; `missing` is completed by cross product.
    Complete { missing: usize },
    /// Only the listed axes are measured; the map cannot be completed.
    Partial { moving: Vec<usize> },
}

/// Peak velocity (mm/s) a healthy segment on `axis` reaches: `min(speed, sqrt(accel * 30))`.
///
/// Z is also capped by `max_z_velocity`, and `max_z_accel` replaces `accel` when it is lower.
/// Returns `None` for Z when either Z limit is unknown, because Z then has no floor.
pub(crate) fn expected_peak(axis: usize, p: &AxesMapParams) -> Option<f64> {
    if axis != 2 {
        return Some(p.speed.min((p.accel * SEGMENT_LENGTH_MM).sqrt()));
    }
    let (velocity, accel) = (p.max_z_velocity?, p.max_z_accel?);
    let accel = p.accel.min(accel);
    Some(
        p.speed
            .min(velocity)
            .min((accel * SEGMENT_LENGTH_MM).sqrt()),
    )
}

/// Whether the segment's largest peak is under the floor; `None` when the axis has no floor.
fn under_floor(axis: usize, seg: &SegmentResult, p: &AxesMapParams) -> Option<bool> {
    let expected = expected_peak(axis, p)?;
    let largest = seg
        .peak_velocities
        .iter()
        .fold(0.0_f64, |m, v| m.max(v.abs()));
    Some(largest < FLOOR_FRACTION * expected)
}

/// A weak segment is noise-only when another segment has the chip axis it claims and a peak
/// four times as large.
const REPEAT_FRACTION: f64 = 0.25;

/// The segment that only repeats a chip axis another segment has, with a peak under
/// `REPEAT_FRACTION` of that segment's, when exactly one pair of segments claims the same chip
/// axis. Shake&Tune reports "same axis detected" there. A toolhead chip on a printer whose bed
/// moves Z feels almost nothing during the Z move, and a small wiggle on a horizontal chip axis
/// can reach the confidence the relative rule asks for, so Z would take X's axis and the run
/// would fail. Two strong segments on one axis still fail, as does a third on the same axis.
fn weak_repeat(seg: &[SegmentResult; 3], peaks: [f64; 3]) -> Option<usize> {
    let claimed: [usize; 3] =
        std::array::from_fn(|i| super::quality::dominant_index(&seg[i].direction_vector));
    let pairs: Vec<(usize, usize)> = [(0, 1), (0, 2), (1, 2)]
        .into_iter()
        .filter(|&(i, j)| claimed[i] == claimed[j])
        .collect();
    let [(i, j)] = pairs.as_slice() else {
        return None;
    };
    let (weaker, stronger) = if peaks[*i] < peaks[*j] {
        (*i, *j)
    } else {
        (*j, *i)
    };
    (peaks[weaker] < REPEAT_FRACTION * peaks[stronger]).then_some(weaker)
}

/// Decide which of the three printer axes carry motion.
///
/// # Errors
/// `InvalidParameter` when a toolhead chip has two or more noise-only axes.
pub(crate) fn classify(
    mounting: Mounting,
    seg: &[SegmentResult; 3],
    p: &AxesMapParams,
) -> Result<MotionCase, CoreError> {
    let under: [Option<bool>; 3] = std::array::from_fn(|i| under_floor(i, &seg[i], p));
    let xy_under = under[0] == Some(true) && under[1] == Some(true);

    // D27 (N1): the relative rule marks nothing when every segment is noise, so a chip that
    // never moved would get a map built from noise. X and Y under the floor with Z under it,
    // or with no Z floor to judge by, is "no motion seen" for both mountings.
    if xy_under && under[2] != Some(false) {
        return Ok(MotionCase::NoMotion);
    }

    match mounting {
        Mounting::Toolhead => {
            let confidences = seg.each_ref().map(|s| s.confidence);
            let peaks = seg.each_ref().map(|s| {
                s.peak_velocities
                    .iter()
                    .fold(0.0_f64, |m, v| m.max(v.abs()))
            });
            Ok(
                match super::orientation::relative_noise_only_axis(confidences, peaks)?
                    .or_else(|| weak_repeat(seg, peaks))
                {
                    Some(missing) => MotionCase::Complete { missing },
                    None => MotionCase::Full,
                },
            )
        }
        Mounting::Bed => {
            let flags = under.map(|u| u == Some(true));
            Ok(match under[2] {
                // Unknown Z limit: only X and Y decide.
                None => match (flags[0], flags[1]) {
                    (false, false) => MotionCase::Complete { missing: 2 },
                    (false, true) => MotionCase::Partial { moving: vec![0] },
                    (true, false) => MotionCase::Partial { moving: vec![1] },
                    (true, true) => MotionCase::NoMotion,
                },
                Some(_) => {
                    let moving: Vec<usize> = (0..3).filter(|&i| !flags[i]).collect();
                    match moving.len() {
                        3 => MotionCase::Full,
                        2 => MotionCase::Complete {
                            missing: (0..3).find(|&i| flags[i]).unwrap_or(2),
                        },
                        1 => MotionCase::Partial { moving },
                        _ => MotionCase::NoMotion,
                    }
                }
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axes_map::segment::SegmentResult;
    use crate::axes_map::{
        AxesMapInput, AxesMapParams, AxesMapResult, AxisState, MapState, Mounting, QualityStatus,
        SegmentCurves, analyze_axes_map,
    };
    use crate::error::CoreError;
    use crate::samples::RawSamples;

    const RATE: f64 = 3200.0;
    const UNDER: f64 = 1.0;
    const ABOVE: f64 = 80.0;

    fn params(z_velocity: Option<f64>, z_accel: Option<f64>) -> AxesMapParams {
        AxesMapParams {
            speed: 80.0,
            accel: 3000.0,
            max_z_velocity: z_velocity,
            max_z_accel: z_accel,
        }
    }

    fn known() -> AxesMapParams {
        params(Some(100.0), Some(3000.0))
    }

    fn unknown() -> AxesMapParams {
        params(None, None)
    }

    /// A segment as `process_segment` would report it, with the given peaks on chip axes.
    fn seg(peaks: [f64; 3], confidence: f64) -> SegmentResult {
        SegmentResult {
            direction_vector: [1.0, 0.0, 0.0],
            actual_direction: [1.0, 0.0, 0.0],
            angle_error: 0.0,
            confidence,
            noise_level: 0.0,
            peak_velocities: peaks,
            gravity: 9810.0,
            peak_time_s: 0.0,
            curves: SegmentCurves {
                t: vec![],
                velocity: [vec![], vec![], vec![]],
            },
        }
    }

    fn on_axis(axis: usize, peak: f64) -> SegmentResult {
        let mut peaks = [0.0; 3];
        peaks[axis] = peak;
        seg(peaks, 1.0)
    }

    /// Samples of a trapezoid velocity profile: `peak` mm/s on `axis` (None: noise only),
    /// with gravity on chip Z and a small deterministic ripple on every axis.
    fn trapezoid(axis: Option<usize>, peak: f64) -> RawSamples {
        let n = (0.6 * RATE) as usize;
        let ramp = 0.1;
        let mut cols = [vec![0.0; n], vec![0.0; n], vec![9810.0; n]];
        let mut state = 12345_u32;
        for i in 0..n {
            let t = i as f64 / RATE;
            for col in &mut cols {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                col[i] += (f64::from(state >> 16) / 65536.0 - 0.5) * 100.0;
            }
            if let Some(k) = axis {
                let a = peak / ramp;
                cols[k][i] += if t < ramp {
                    a
                } else if t > 0.3 && t < 0.3 + ramp {
                    -a
                } else {
                    0.0
                };
            }
        }
        let [x, y, z] = cols;
        RawSamples {
            t: (0..n).map(|i| i as f64 / RATE).collect(),
            x,
            y,
            z,
        }
    }

    fn analyze(
        mounting: Mounting,
        sweeps: [(Option<usize>, f64); 3],
        p: &AxesMapParams,
    ) -> AxesMapResult {
        let input = AxesMapInput {
            chip: "adxl345".to_owned(),
            segments: sweeps.map(|(axis, peak)| trapezoid(axis, peak)),
            windows: [None, None, None],
            configured_map: None,
            mounting,
        };
        analyze_axes_map(&input, p).unwrap()
    }

    const ALL_THREE: [(Option<usize>, f64); 3] =
        [(Some(0), ABOVE), (Some(1), ABOVE), (Some(2), ABOVE)];

    /// A segment that claims chip axis `claimed`, as `process_segment` reports the dominant one.
    fn claiming(claimed: usize, peaks: [f64; 3], confidence: f64) -> SegmentResult {
        let mut s = seg(peaks, confidence);
        s.direction_vector = [0.0; 3];
        s.direction_vector[claimed] = 1.0;
        s
    }

    #[test]
    fn a_weak_segment_that_repeats_a_chip_axis_is_noise_only_for_a_toolhead_chip() {
        // The weak Z has 34 % confidence, so the relative rule keeps it, and it matches the chip
        // axis that X already has.
        let segs = [
            claiming(0, [80.0, 2.0, 1.0], 1.0),
            claiming(1, [3.0, 80.0, 1.0], 1.0),
            claiming(0, [10.0, 3.0, 4.0], 0.34),
        ];
        assert_eq!(
            classify(Mounting::Toolhead, &segs, &known()),
            Ok(MotionCase::Complete { missing: 2 })
        );
    }

    #[test]
    fn the_weak_repeating_segment_can_be_any_of_the_three() {
        let x_weak = [
            claiming(1, [3.0, 10.0, 1.0], 0.34),
            claiming(1, [3.0, 80.0, 1.0], 1.0),
            claiming(2, [1.0, 1.0, 80.0], 1.0),
        ];
        assert_eq!(
            classify(Mounting::Toolhead, &x_weak, &known()),
            Ok(MotionCase::Complete { missing: 0 })
        );
        let y_weak = [
            claiming(0, [80.0, 2.0, 1.0], 1.0),
            claiming(2, [1.0, 1.0, 10.0], 0.34),
            claiming(2, [1.0, 1.0, 80.0], 1.0),
        ];
        assert_eq!(
            classify(Mounting::Toolhead, &y_weak, &known()),
            Ok(MotionCase::Complete { missing: 1 })
        );
    }

    #[test]
    fn two_strong_segments_that_repeat_a_chip_axis_are_not_noise() {
        // A chip that really is mounted so that two moves look alike keeps the error.
        let segs = [
            claiming(0, [80.0, 2.0, 1.0], 1.0),
            claiming(1, [3.0, 80.0, 1.0], 1.0),
            claiming(0, [60.0, 3.0, 4.0], 0.9),
        ];
        assert_eq!(
            classify(Mounting::Toolhead, &segs, &known()),
            Ok(MotionCase::Full)
        );
    }

    #[test]
    fn a_repeating_peak_at_a_quarter_of_the_stronger_one_is_not_noise() {
        let at_a_quarter = [
            claiming(0, [80.0, 2.0, 1.0], 1.0),
            claiming(1, [3.0, 80.0, 1.0], 1.0),
            claiming(0, [20.0, 3.0, 4.0], 0.34),
        ];
        assert_eq!(
            classify(Mounting::Toolhead, &at_a_quarter, &known()),
            Ok(MotionCase::Full)
        );
    }

    #[test]
    fn three_segments_on_one_chip_axis_are_left_to_the_error() {
        let segs = [
            claiming(0, [80.0, 2.0, 1.0], 1.0),
            claiming(0, [70.0, 3.0, 1.0], 1.0),
            claiming(0, [10.0, 3.0, 4.0], 0.34),
        ];
        assert_eq!(
            classify(Mounting::Toolhead, &segs, &known()),
            Ok(MotionCase::Full)
        );
    }

    #[test]
    fn a_weak_z_that_repeats_the_x_axis_is_inferred_for_a_toolhead_chip() {
        let result = analyze(
            Mounting::Toolhead,
            [(Some(0), ABOVE), (Some(1), ABOVE), (Some(0), 10.0)],
            &known(),
        );
        assert_eq!(result.state, MapState::Complete);
        assert_eq!(result.map.as_deref(), Some("x, y, z"));
        assert_eq!(result.axes[2].state, AxisState::Virtual);
    }

    #[test]
    fn a_strong_z_that_repeats_the_x_axis_is_still_an_error() {
        let result = analyze(
            Mounting::Toolhead,
            [(Some(0), ABOVE), (Some(1), ABOVE), (Some(0), 60.0)],
            &known(),
        );
        assert_eq!(result.state, MapState::Ambiguous);
        assert_eq!(result.quality.status, QualityStatus::Error);
    }

    #[test]
    fn guard_rejects_a_toolhead_chip_that_never_moved() {
        // Noise on every segment: the relative rule alone would mark nothing and build a map.
        let segs = [
            seg([2.0, 1.0, 0.5], 0.1),
            seg([0.5, 2.0, 1.0], 0.1),
            seg([1.0, 0.5, 2.0], 0.1),
        ];
        assert_eq!(
            classify(Mounting::Toolhead, &segs, &known()),
            Ok(MotionCase::NoMotion)
        );
    }

    #[test]
    fn zero_peaks_on_all_three_are_no_motion_with_an_unknown_z_limit() {
        for mounting in [Mounting::Toolhead, Mounting::Bed] {
            let result = analyze(mounting, [(None, 0.0); 3], &unknown());
            assert_eq!(result.state, MapState::NoMotion);
            assert_eq!(result.map, None);
            assert_eq!(result.euler_deg, None);
            assert_eq!(result.rotation, None);
            assert!(
                result
                    .axes
                    .iter()
                    .all(|a| a.state == AxisState::NotDetermined),
                "{mounting:?}"
            );
        }
    }

    #[test]
    fn comparable_noise_on_all_three_is_no_motion_with_an_unknown_z_limit() {
        let segs = [
            seg([3.0, 2.0, 3.0], 0.1),
            seg([2.0, 3.0, 2.0], 0.1),
            seg([3.0, 3.0, 2.0], 0.1),
        ];
        for mounting in [Mounting::Toolhead, Mounting::Bed] {
            assert_eq!(
                classify(mounting, &segs, &unknown()),
                Ok(MotionCase::NoMotion)
            );
        }
    }

    #[test]
    fn bed_slinger_bed_chip_with_dominant_noise_on_z_is_partial_without_a_snippet() {
        // Only the Y sweep moves the bed chip; the Z segment's noise is dominated by one axis.
        let result = analyze(
            Mounting::Bed,
            [(None, 0.0), (Some(1), ABOVE), (None, 0.0)],
            &unknown(),
        );
        assert_eq!(result.state, MapState::Partial);
        assert_eq!(result.map, None);
        assert_eq!(result.euler_deg, None);
        assert_eq!(result.rotation, None);
        assert_eq!(result.axes[0].state, AxisState::NotDetermined);
        assert_eq!(result.axes[1].state, AxisState::Measured);
        assert_eq!(result.axes[2].state, AxisState::NotDetermined);
    }

    #[test]
    fn toolhead_chip_with_dominant_z_noise_and_an_unknown_z_limit_is_no_motion() {
        // Z noise is dominated by one axis, so its confidence is high; X and Y saw nothing.
        let segs = [
            seg([2.0, 1.0, 1.0], 0.1),
            seg([1.0, 2.0, 1.0], 0.1),
            seg([0.2, 0.1, 9.0], 1.0),
        ];
        assert_eq!(
            classify(Mounting::Toolhead, &segs, &unknown()),
            Ok(MotionCase::NoMotion)
        );
    }

    #[test]
    fn a_z_only_bed_chip_with_an_unknown_z_limit_is_no_motion_with_the_z_limit_message() {
        let result = analyze(
            Mounting::Bed,
            [(None, 0.0), (None, 0.0), (Some(2), ABOVE)],
            &unknown(),
        );
        assert_eq!(result.state, MapState::NoMotion);
        assert_eq!(result.map, None);
        assert_eq!(result.quality.status, QualityStatus::Error);
        assert!(
            result
                .quality
                .messages
                .iter()
                .any(|m| m.contains("Z limits")),
            "{:?}",
            result.quality.messages
        );
    }

    #[test]
    fn bed_chip_moving_on_x_and_y_completes_z_as_virtual() {
        for p in [known(), unknown()] {
            let result = analyze(
                Mounting::Bed,
                [(Some(0), ABOVE), (Some(1), ABOVE), (None, 0.0)],
                &p,
            );
            assert_eq!(result.state, MapState::Complete);
            assert_eq!(result.axes[0].state, AxisState::Measured);
            assert_eq!(result.axes[1].state, AxisState::Measured);
            assert_eq!(result.axes[2].state, AxisState::Virtual);
            assert_eq!(result.map.as_deref(), Some("-x, -y, z"));
        }
    }

    #[test]
    fn completed_z_does_not_depend_on_the_flip() {
        let sweeps = [(Some(0), ABOVE), (Some(1), ABOVE), (None, 0.0)];
        let toolhead = analyze(Mounting::Toolhead, sweeps, &known());
        let bed = analyze(Mounting::Bed, sweeps, &known());
        assert_eq!(toolhead.axes[2].chip_axis, bed.axes[2].chip_axis);
        assert_eq!(toolhead.axes[2].negative, bed.axes[2].negative);
        assert_eq!(toolhead.map.as_deref(), Some("x, y, z"));
    }

    #[test]
    fn bed_chip_flips_the_measured_signs() {
        let toolhead = analyze(Mounting::Toolhead, ALL_THREE, &known());
        let bed = analyze(Mounting::Bed, ALL_THREE, &known());
        assert_eq!(toolhead.map.as_deref(), Some("x, y, z"));
        assert_eq!(bed.map.as_deref(), Some("-x, -y, -z"));
        for i in 0..3 {
            assert_ne!(toolhead.axes[i].negative, bed.axes[i].negative);
        }
    }

    #[test]
    fn one_two_and_three_segments_under_the_floor_each_with_known_and_unknown_z() {
        use MotionCase::{Complete, Full, NoMotion, Partial};
        // (under the floor, known Z limits, unknown Z limits)
        let table = [
            ([false, false, false], Full, Complete { missing: 2 }),
            (
                [false, true, false],
                Complete { missing: 1 },
                Partial { moving: vec![0] },
            ),
            (
                [false, false, true],
                Complete { missing: 2 },
                Complete { missing: 2 },
            ),
            ([true, true, false], Partial { moving: vec![2] }, NoMotion),
            (
                [true, false, true],
                Partial { moving: vec![1] },
                Partial { moving: vec![1] },
            ),
            ([true, true, true], NoMotion, NoMotion),
        ];
        for (under, with_known, with_unknown) in table {
            let segs: [SegmentResult; 3] =
                std::array::from_fn(|i| on_axis(i, if under[i] { UNDER } else { ABOVE }));
            assert_eq!(
                classify(Mounting::Bed, &segs, &known()).unwrap(),
                with_known,
                "known, under {under:?}"
            );
            assert_eq!(
                classify(Mounting::Bed, &segs, &unknown()).unwrap(),
                with_unknown,
                "unknown, under {under:?}"
            );
        }
    }

    #[test]
    fn a_z_signal_limited_by_acceleration_is_still_above_the_floor() {
        let p = params(Some(100.0), Some(10.0));
        let expected = expected_peak(2, &p).unwrap();
        assert!((expected - 300.0_f64.sqrt()).abs() < 1e-9, "{expected}");
        // 5 mm/s is above a quarter of 17.3, 4 mm/s is below it.
        let segs = |z: f64| [on_axis(0, ABOVE), on_axis(1, ABOVE), on_axis(2, z)];
        assert_eq!(
            classify(Mounting::Bed, &segs(5.0), &p),
            Ok(MotionCase::Full)
        );
        assert_eq!(
            classify(Mounting::Bed, &segs(4.0), &p),
            Ok(MotionCase::Complete { missing: 2 })
        );
    }

    #[test]
    fn above_floor_duplicate_matches_are_ambiguous_on_a_bed_chip() {
        // The Y sweep shows up on chip X, like the X sweep.
        let result = analyze(
            Mounting::Bed,
            [(Some(0), ABOVE), (Some(0), ABOVE), (Some(2), ABOVE)],
            &known(),
        );
        assert_eq!(result.state, MapState::Ambiguous);
        assert_eq!(result.map, None);
        assert_eq!(result.euler_deg, None);
        assert_eq!(result.rotation, None);
    }

    #[test]
    fn two_relative_noise_only_axes_on_a_toolhead_chip_are_an_error_with_the_reference_text() {
        let segs = [
            on_axis(0, ABOVE),
            seg([5.0, 1.0, 1.0], 0.0),
            seg([1.0, 1.0, 5.0], 0.0),
        ];
        assert_eq!(
            classify(Mounting::Toolhead, &segs, &unknown()),
            Err(CoreError::InvalidParameter(
                "Multiple axes (2) have no signal. Ensure accelerometer is properly mounted \
                 and moves with toolhead."
                    .to_owned()
            ))
        );
    }
}
