//! The worst case of the vibrations profile over several chips.

use wobblescope_core::vibrations::{Grid, Kinematics, VibrationsParams, worst_case};

fn params() -> VibrationsParams {
    VibrationsParams {
        max_freq: 1000.0,
        kinematics: Kinematics::Cartesian,
    }
}

/// A grid of 720 angles and 12 speeds with a value that depends on `seed`, angle and speed.
fn grid(seed: f64) -> Grid {
    let angles: Vec<f64> = (0..720).map(|i| i as f64 * 360.0 / 719.0).collect();
    let speeds: Vec<f64> = (0..12).map(|i| 20.0 + i as f64 * 5.0).collect();
    let data = (0..720)
        .map(|a| {
            (0..12)
                .map(|s| 1.0 + ((a as f64 * 0.05 + s as f64 * seed).sin() + 1.0) * seed)
                .collect()
        })
        .collect();
    Grid {
        angles,
        speeds,
        data,
    }
}

#[test]
fn worst_case_of_none_is_none() {
    assert!(worst_case(&[], &params()).is_none());
}

#[test]
fn worst_case_is_the_elementwise_maximum() {
    let (a, b) = (grid(1.0), grid(2.5));
    let got = worst_case(&[&a, &b], &params()).unwrap();
    for r in 0..720 {
        for c in 0..12 {
            assert_eq!(
                got.grid[r][c],
                a.data[r][c].max(b.data[r][c]),
                "cell {r},{c}"
            );
        }
    }
    assert_eq!(got.speeds, a.speeds);
    assert_eq!(got.angles, a.angles);
    assert!(got.motor.is_none());
    assert!(got.steps.is_empty());
}

#[test]
fn worst_case_of_one_chip_equals_that_chip() {
    let a = grid(1.7);
    let got = worst_case(&[&a], &params()).unwrap();
    assert_eq!(got.grid, a.data);
    assert_eq!(
        got.angle_energy,
        wobblescope_core::vibrations::angle_powers(&a)
    );
    let curves = wobblescope_core::vibrations::speed_powers(&a);
    assert_eq!(got.speed_min, curves.min);
    assert_eq!(got.speed_max, curves.max);
    assert_eq!(got.speed_variance, curves.variance);
    assert_eq!(got.vibration_metric, curves.metric);
    let want = wobblescope_core::vibrations::symmetry(&a, Kinematics::Cartesian.main_angles());
    assert_eq!(got.symmetry.is_some(), !want.is_nan());
    if let Some(s) = got.symmetry {
        assert_eq!(s, want);
    }
}
