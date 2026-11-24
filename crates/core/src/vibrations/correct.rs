//! The model corrected by measured directions.
//!
//! The model gives an energy for any angle and speed. At every measured angle the correction
//! scales it by a ratio, shifted by a small constant so a zero or tiny model cannot blow it up,
//! and the ratio is interpolated across angle between the measured angles. The result is a
//! continuous function equal to the measurement at every measured angle and speed. The grid is
//! its sampling at the model's rows, so no row is forced to a measurement: a row lies within a
//! quarter of a degree of a measured angle, not on it.

use super::grid::{Grid, Model, interpolated, row_angles};
use crate::error::CoreError;

/// Share of the largest energy of the chip that is added to model and measurement before their
/// ratio is taken.
const FLOOR_SHARE: f64 = 1e-3;
/// Directions repeat every 180 degrees.
const PERIOD_DEGREES: f64 = 180.0;
const NO_ENERGY: &str = "no usable vibration energy";

/// The energy recorded at one angle: `energy[i]` at `speeds[i]`, with the speeds ascending.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    pub angle: f64,
    pub speeds: Vec<f64>,
    pub energy: Vec<f64>,
}

struct Correction<'a> {
    model: &'a Model,
    /// Measured angles reduced to [0, 180), ascending and distinct.
    anchors: Vec<(f64, &'a Measured)>,
    /// The constant added to model and measurement. Zero when every energy is zero.
    floor: f64,
}

fn no_energy() -> CoreError {
    CoreError::InvalidParameter(NO_ENERGY.into())
}

impl<'a> Correction<'a> {
    fn new(
        model: &'a Model,
        extras: &'a [Measured],
        motor: &'a [Measured],
    ) -> Result<Self, CoreError> {
        let Some(reference) = motor.first() else {
            return Err(CoreError::InvalidParameter(
                "the correction needs the motor angles".into(),
            ));
        };
        if extras.is_empty() {
            return Err(CoreError::InvalidParameter(
                "the correction needs at least one extra angle".into(),
            ));
        }
        let mut anchors: Vec<(f64, &Measured)> = Vec::new();
        for measured in motor.iter().chain(extras) {
            if measured.speeds.len() < 2 || measured.speeds.len() != measured.energy.len() {
                return Err(CoreError::InvalidParameter(format!(
                    "the energy at {} degrees needs two or more speeds with an energy each",
                    measured.angle
                )));
            }
            if measured.speeds != reference.speeds {
                return Err(CoreError::InvalidParameter(format!(
                    "the angle {} degrees was not recorded at the speeds of the motor angles",
                    measured.angle
                )));
            }
            if !measured.angle.is_finite()
                || measured.speeds.iter().any(|v| !v.is_finite())
                || measured.energy.iter().any(|v| !v.is_finite())
            {
                return Err(no_energy());
            }
            let angle = measured.angle.rem_euclid(PERIOD_DEGREES);
            if !anchors.iter().any(|(a, _)| *a == angle) {
                anchors.push((angle, measured));
            }
        }
        anchors.sort_by(|a, b| a.0.total_cmp(&b.0));

        let mut largest = f64::NEG_INFINITY;
        for (angle, measured) in &anchors {
            for (speed, energy) in measured.speeds.iter().zip(&measured.energy) {
                let modelled = model.value(*angle, *speed);
                if !modelled.is_finite() {
                    return Err(no_energy());
                }
                largest = largest.max(modelled).max(*energy);
            }
        }
        let floor = if largest > 0.0 {
            FLOOR_SHARE * largest
        } else {
            0.0
        };
        Ok(Self {
            model,
            anchors,
            floor,
        })
    }

    fn anchor_ratio(&self, anchor: usize, speed: f64) -> f64 {
        let (angle, measured) = self.anchors[anchor];
        let m = interpolated(&measured.speeds, &measured.energy, speed);
        let g = self.model.value(angle, speed).max(0.0);
        (m + self.floor) / (g + self.floor)
    }

    /// The ratio at any angle and speed: linear between the two anchors around the angle, wrapping
    /// from the last anchor to the first one 180 degrees on.
    fn ratio(&self, angle: f64, speed: f64) -> f64 {
        let a = angle.rem_euclid(PERIOD_DEGREES);
        let n = self.anchors.len();
        let after = self.anchors.partition_point(|(anchor, _)| *anchor <= a);
        let (previous, next) = ((after + n - 1) % n, after % n);
        let mut from = self.anchors[previous].0;
        let mut to = self.anchors[next].0;
        if after == 0 {
            from -= PERIOD_DEGREES;
        }
        if after == n {
            to += PERIOD_DEGREES;
        }
        let t = (a - from) / (to - from);
        let (r_from, r_to) = (
            self.anchor_ratio(previous, speed),
            self.anchor_ratio(next, speed),
        );
        r_from + t * (r_to - r_from)
    }

    fn value(&self, angle: f64, speed: f64) -> f64 {
        let g = self.model.value(angle, speed).max(0.0);
        if self.floor == 0.0 {
            return g;
        }
        ((g + self.floor) * self.ratio(angle, speed) - self.floor).max(0.0)
    }
}

/// The corrected energy at any angle and speed, clamped to at least 0.
///
/// `motor` holds the measurements at the two motor angles and `extras` those at other angles, all
/// recorded at the same speeds. At a measured angle and a recorded speed the value equals the
/// measurement. When every energy is zero the value is the model's.
///
/// # Errors
/// [`CoreError::InvalidParameter`] when `extras` is empty, when an extra was not recorded at the
/// speeds of the motor angles, when a curve has fewer than two speeds or lists of different
/// lengths, and with "no usable vibration energy" when a figure is not finite.
pub fn corrected_value(
    model: &Model,
    extras: &[Measured],
    motor: &[Measured],
    angle: f64,
    speed: f64,
) -> Result<f64, CoreError> {
    Ok(Correction::new(model, extras, motor)?.value(angle, speed))
}

/// The corrected function sampled at the model's 720 rows and its speed axis.
///
/// # Errors
/// As [`corrected_value`].
pub fn corrected_grid(
    model: &Model,
    extras: &[Measured],
    motor: &[Measured],
) -> Result<Grid, CoreError> {
    let correction = Correction::new(model, extras, motor)?;
    let angles = row_angles();
    let speeds = model.speed_axis();
    let data = angles
        .iter()
        .map(|angle| {
            speeds
                .iter()
                .map(|speed| correction.value(*angle, *speed))
                .collect()
        })
        .collect();
    Ok(Grid {
        angles,
        speeds,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vibrations::grid::{EnergyByAngleSpeed, Kinematics};

    const SPEEDS: [f64; 5] = [20.0, 30.0, 40.0, 50.0, 60.0];

    fn motor_energy() -> EnergyByAngleSpeed {
        let curve = |scale: f64| -> Vec<(f64, f64)> {
            SPEEDS.iter().map(|s| (*s, scale * (10.0 + s))).collect()
        };
        vec![(0.0, curve(1.0)), (90.0, curve(2.0))]
    }

    fn model() -> Model {
        Model::new(&motor_energy(), Kinematics::Cartesian, [0.0, 90.0])
    }

    /// The model's own values at the recorded speeds, which is what an exact model measures.
    fn modelled(model: &Model, angle: f64) -> Measured {
        Measured {
            angle,
            speeds: SPEEDS.to_vec(),
            energy: SPEEDS.iter().map(|s| model.value(angle, *s)).collect(),
        }
    }

    fn motor(model: &Model) -> Vec<Measured> {
        vec![modelled(model, 0.0), modelled(model, 90.0)]
    }

    fn scaled(model: &Model, angle: f64, factor: f64) -> Measured {
        let mut m = modelled(model, angle);
        m.energy.iter_mut().for_each(|e| *e *= factor);
        m
    }

    #[test]
    fn correction_equals_the_measurement_at_a_measured_angle() {
        let model = model();
        let extra = scaled(&model, 22.5, 2.0);
        for (speed, want) in SPEEDS.iter().zip(&extra.energy) {
            let got = corrected_value(
                &model,
                std::slice::from_ref(&extra),
                &motor(&model),
                22.5,
                *speed,
            )
            .unwrap();
            assert!((got - want).abs() < 1e-9, "{speed}: {got} vs {want}");
        }
    }

    #[test]
    fn neighbours_move_toward_the_measurement() {
        let model = model();
        let extra = scaled(&model, 22.5, 2.0);
        let extras = std::slice::from_ref(&extra);
        for angle in [15.0, 30.0] {
            let got = corrected_value(&model, extras, &motor(&model), angle, 40.0).unwrap();
            let plain = model.value(angle, 40.0);
            assert!(
                got > plain * 1.01 && got < plain * 2.0,
                "{angle}: {got} vs {plain}"
            );
        }
        let grid = corrected_grid(&model, extras, &motor(&model)).unwrap();
        assert!(![15.0, 22.5, 30.0].iter().any(|a| grid.angles.contains(a)));
    }

    #[test]
    fn an_exact_model_leaves_the_grid_unchanged() {
        let model = model();
        let extras = [modelled(&model, 22.5), modelled(&model, 45.0)];
        let motor = motor(&model);
        let corrected = Correction::new(&model, &extras, &motor).unwrap();
        for angle in [0.0, 10.0, 22.5, 70.0, 135.0, 200.0, 359.0] {
            for speed in SPEEDS {
                let want = model.value(angle, speed).max(0.0);
                let got = corrected.value(angle, speed);
                assert!(
                    (got - want).abs() < 1e-9,
                    "{angle} {speed}: {got} vs {want}"
                );
            }
        }
    }

    #[test]
    fn ratios_stay_between_their_neighbours_and_across_the_seam() {
        let model = model();
        let extras = [scaled(&model, 22.5, 3.0), scaled(&model, 135.0, 0.5)];
        let motor = motor(&model);
        let correction = Correction::new(&model, &extras, &motor).unwrap();
        let ratio_at = |angle: f64| correction.ratio(angle, 40.0);
        // Anchors at 0, 22.5, 90, 135 and 180 (= 0).
        for (low, high, from, to) in [
            (0.0, 22.5, 0.0, 22.5),
            (22.5, 90.0, 22.5, 90.0),
            (90.0, 135.0, 90.0, 135.0),
            (135.0, 180.0, 135.0, 180.0),
        ] {
            let (a, b) = (ratio_at(from), ratio_at(to));
            let (lo, hi) = (a.min(b), a.max(b));
            for step in 1..10 {
                let angle = low + (high - low) * f64::from(step) / 10.0;
                let r = ratio_at(angle);
                assert!(
                    r >= lo - 1e-12 && r <= hi + 1e-12,
                    "{angle}: {r} not in {lo}..{hi}"
                );
            }
        }
        for angle in [0.0, 22.5, 90.0, 135.0] {
            let step = 1e-9;
            assert!((ratio_at(angle - step) - ratio_at(angle + step)).abs() < 1e-6);
            assert!((ratio_at(angle) - ratio_at(angle + 180.0)).abs() < 1e-12);
        }
        assert!((ratio_at(-1e-9) - ratio_at(1e-9)).abs() < 1e-6);
        assert!((ratio_at(180.0 - 1e-9) - ratio_at(180.0 + 1e-9)).abs() < 1e-6);
    }

    #[test]
    fn zero_model_with_positive_measurement_is_finite() {
        let zero: EnergyByAngleSpeed = vec![
            (0.0, SPEEDS.iter().map(|s| (*s, 0.0)).collect()),
            (90.0, SPEEDS.iter().map(|s| (*s, 0.0)).collect()),
        ];
        let model = Model::new(&zero, Kinematics::Cartesian, [0.0, 90.0]);
        let extra = Measured {
            angle: 45.0,
            speeds: SPEEDS.to_vec(),
            energy: vec![5.0; 5],
        };
        let grid = corrected_grid(&model, &[extra], &motor(&model)).unwrap();
        assert!(
            grid.data
                .iter()
                .flatten()
                .all(|v| v.is_finite() && *v >= 0.0)
        );
    }

    #[test]
    fn positive_model_with_zero_measurement_is_finite() {
        let model = model();
        let extra = Measured {
            angle: 45.0,
            speeds: SPEEDS.to_vec(),
            energy: vec![0.0; 5],
        };
        let grid = corrected_grid(&model, &[extra], &motor(&model)).unwrap();
        assert!(
            grid.data
                .iter()
                .flatten()
                .all(|v| v.is_finite() && *v >= 0.0)
        );
    }

    #[test]
    fn all_zero_is_the_identity() {
        let zero: EnergyByAngleSpeed = vec![
            (0.0, SPEEDS.iter().map(|s| (*s, 0.0)).collect()),
            (90.0, SPEEDS.iter().map(|s| (*s, 0.0)).collect()),
        ];
        let model = Model::new(&zero, Kinematics::Cartesian, [0.0, 90.0]);
        let extra = Measured {
            angle: 45.0,
            speeds: SPEEDS.to_vec(),
            energy: vec![0.0; 5],
        };
        let grid = corrected_grid(&model, &[extra], &motor(&model)).unwrap();
        assert!(grid.data.iter().flatten().all(|v| *v == 0.0));
    }

    #[test]
    fn negative_model_is_clamped() {
        // A rising curve extrapolates below zero at speeds under the first one.
        let squares = |scale: f64| -> Vec<(f64, f64)> {
            SPEEDS
                .iter()
                .map(|s| (*s, scale * (s / 10.0).powi(2)))
                .collect()
        };
        let energy: EnergyByAngleSpeed = vec![(0.0, squares(1.0)), (90.0, squares(2.0))];
        let model = Model::new(&energy, Kinematics::Cartesian, [0.0, 90.0]);
        assert!(
            model.value(0.0, 20.0) < 0.0,
            "the fixture needs a negative model"
        );
        let extra = Measured {
            angle: 45.0,
            speeds: SPEEDS.to_vec(),
            energy: SPEEDS.iter().map(|s| s / 10.0).collect(),
        };
        let motor: Vec<Measured> = [0.0, 90.0]
            .iter()
            .map(|angle| Measured {
                angle: *angle,
                speeds: SPEEDS.to_vec(),
                energy: SPEEDS.iter().map(|s| s / 5.0).collect(),
            })
            .collect();
        let grid = corrected_grid(&model, &[extra], &motor).unwrap();
        assert!(
            grid.data
                .iter()
                .flatten()
                .all(|v| v.is_finite() && *v >= 0.0)
        );
    }

    #[test]
    fn a_non_finite_input_is_an_error() {
        let model = model();
        let mut extra = modelled(&model, 22.5);
        extra.energy[2] = f64::NAN;
        let err = corrected_value(&model, &[extra], &motor(&model), 10.0, 30.0).unwrap_err();
        assert!(
            err.to_string().contains("no usable vibration energy"),
            "{err}"
        );
        let mut extra = modelled(&model, 22.5);
        extra.energy[2] = f64::INFINITY;
        assert!(corrected_grid(&model, &[extra], &motor(&model)).is_err());
    }

    #[test]
    fn extras_at_other_speeds_are_refused() {
        let model = model();
        let mut extra = modelled(&model, 22.5);
        extra.speeds[2] += 1.0;
        assert!(matches!(
            corrected_grid(&model, &[extra], &motor(&model)),
            Err(CoreError::InvalidParameter(_))
        ));
        assert!(matches!(
            corrected_grid(&model, &[], &motor(&model)),
            Err(CoreError::InvalidParameter(_))
        ));
    }
}
