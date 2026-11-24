//! Scoring the correction on held-out directions.
//!
//! For each extra angle in turn, [`holdout_scores`] predicts it from the plain model and from the
//! model corrected by the other extras, and reports how far each prediction is from the
//! measurement. It needs a capture with measurements at several angles, such as every 15 degrees.
//! A median corrected error of at most 0.9 times the plain model's is a clear gain.

use super::analysis::measured_curves;
use super::correct::{Measured, corrected_value};
use super::grid::{EnergyByAngleSpeed, Kinematics, Model};

/// How far the plain model and the corrected model are from one held-out angle. Both errors are
/// the RMS difference over the recorded speeds relative to the RMS of the measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoldoutScore {
    pub angle: f64,
    pub plain_error: f64,
    pub corrected_error: f64,
}

/// The median of `values`, the mean of the two middle ones for an even count, NaN for none.
pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    }
}

fn rms(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0.0), |(sum, count), v| (sum + v * v, count + 1.0));
    (sum / count).sqrt()
}

fn measured_at(energy: &EnergyByAngleSpeed, angle: f64) -> Option<Measured> {
    measured_curves(energy)
        .into_iter()
        .find(|m| m.angle == angle)
}

/// The score of every extra angle of `energy`, in the order they appear. Both motor angles are
/// always kept, as the model needs them, so only the other angles are held out.
///
/// An angle whose measurement has no energy (zero RMS) is omitted, as is one that cannot be
/// corrected, for instance because it was recorded at other speeds than the motor angles. With no
/// other extras left, the corrected prediction is the plain model's. Without a recording of each
/// motor angle or with fewer than two motor speeds there is no model, and the result is empty.
pub fn holdout_scores(
    energy: &EnergyByAngleSpeed,
    k: Kinematics,
    main: [f64; 2],
) -> Vec<HoldoutScore> {
    let motor: Vec<Measured> = main
        .iter()
        .filter_map(|angle| measured_at(energy, *angle))
        .collect();
    let mut motor_speeds: Vec<f64> = motor
        .iter()
        .flat_map(|m| m.speeds.iter().copied())
        .collect();
    motor_speeds.sort_by(f64::total_cmp);
    motor_speeds.dedup();
    if motor.len() < main.len() || motor_speeds.len() < 2 {
        return Vec::new();
    }
    let model = Model::new(energy, k, main);
    let extras: Vec<Measured> = energy
        .iter()
        .map(|(angle, _)| *angle)
        .filter(|angle| !main.contains(angle))
        .filter_map(|angle| measured_at(energy, angle))
        .collect();

    let mut scores = Vec::new();
    for (index, held) in extras.iter().enumerate() {
        let remaining: Vec<Measured> = extras
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, m)| m.clone())
            .collect();
        let scale = rms(held.energy.iter().copied());
        if scale == 0.0 || !scale.is_finite() {
            continue;
        }
        let plain: Vec<f64> = held
            .speeds
            .iter()
            .map(|s| model.value(held.angle, *s).max(0.0))
            .collect();
        let corrected: Result<Vec<f64>, _> = if remaining.is_empty() {
            Ok(plain.clone())
        } else {
            held.speeds
                .iter()
                .map(|s| corrected_value(&model, &remaining, &motor, held.angle, *s))
                .collect()
        };
        let Ok(corrected) = corrected else { continue };
        let error = |prediction: &[f64]| {
            rms(prediction.iter().zip(&held.energy).map(|(p, m)| p - m)) / scale
        };
        scores.push(HoldoutScore {
            angle: held.angle,
            plain_error: error(&plain),
            corrected_error: error(&corrected),
        });
    }
    scores
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEEDS: [f64; 5] = [20.0, 30.0, 40.0, 50.0, 60.0];
    const MAIN: [f64; 2] = [0.0, 90.0];

    fn motor_only() -> EnergyByAngleSpeed {
        let curve =
            |scale: f64| -> Vec<(f64, f64)> { SPEEDS.iter().map(|s| (*s, scale * s)).collect() };
        vec![(0.0, curve(1.0)), (90.0, curve(2.0))]
    }

    fn with_extras(angles: &[f64], factor: f64) -> EnergyByAngleSpeed {
        let mut energy = motor_only();
        let model = Model::new(&motor_only(), Kinematics::Cartesian, MAIN);
        for angle in angles {
            energy.push((
                *angle,
                SPEEDS
                    .iter()
                    .map(|s| (*s, factor * model.value(*angle, *s)))
                    .collect(),
            ));
        }
        energy
    }

    fn score(energy: &EnergyByAngleSpeed) -> Vec<HoldoutScore> {
        holdout_scores(energy, Kinematics::Cartesian, MAIN)
    }

    #[test]
    fn a_scaled_direction_scores_better_with_the_correction() {
        let scores = score(&with_extras(&[15.0, 22.5, 30.0], 2.0));
        assert_eq!(scores.len(), 3);
        let plain: Vec<f64> = scores.iter().map(|s| s.plain_error).collect();
        let corrected: Vec<f64> = scores.iter().map(|s| s.corrected_error).collect();
        assert!(
            median(&corrected) <= 0.9 * median(&plain),
            "{corrected:?} vs {plain:?}"
        );
    }

    #[test]
    fn an_exact_model_scores_equal() {
        let scores = score(&with_extras(&[15.0, 30.0], 1.0));
        for s in scores {
            assert!(s.plain_error < 1e-12, "{s:?}");
            assert!(s.corrected_error < 1e-9, "{s:?}");
        }
    }

    #[test]
    fn both_motor_angles_are_always_kept() {
        let scores = score(&with_extras(&[15.0, 30.0], 2.0));
        let angles: Vec<f64> = scores.iter().map(|s| s.angle).collect();
        assert_eq!(angles, vec![15.0, 30.0]);
        assert!(!angles.contains(&0.0) && !angles.contains(&90.0));
    }

    #[test]
    fn with_no_extras_the_scores_are_empty() {
        assert!(score(&motor_only()).is_empty());
    }

    #[test]
    fn without_a_motor_angle_or_two_motor_speeds_the_scores_are_empty() {
        let mut missing_motor = with_extras(&[15.0], 2.0);
        missing_motor.retain(|(angle, _)| *angle != 90.0);
        assert!(score(&missing_motor).is_empty());

        let mut one_speed = with_extras(&[15.0], 2.0);
        for (_, curve) in one_speed.iter_mut() {
            curve.truncate(1);
        }
        assert!(score(&one_speed).is_empty());
    }

    #[test]
    fn one_extra_scores_equal_errors() {
        let scores = score(&with_extras(&[22.5], 2.0));
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].plain_error, scores[0].corrected_error);
        assert!(scores[0].plain_error > 0.1);
    }

    #[test]
    fn a_zero_energy_angle_is_omitted() {
        let mut energy = with_extras(&[15.0], 2.0);
        energy.push((30.0, SPEEDS.iter().map(|s| (*s, 0.0)).collect()));
        let angles: Vec<f64> = score(&energy).iter().map(|s| s.angle).collect();
        assert_eq!(angles, vec![15.0]);
    }

    #[test]
    fn median_of_empty_is_nan() {
        assert!(median(&[]).is_nan());
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), 2.5);
    }
}
