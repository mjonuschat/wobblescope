use wobblescope_core::RawSamples;
use wobblescope_core::vibrations::{
    Grid, Kinematics, MeasuredCurve, StepInput, VibrationsParams, analyze_chip, worst_case,
};

fn params() -> VibrationsParams {
    VibrationsParams {
        max_freq: 1000.0,
        kinematics: Kinematics::Cartesian,
    }
}

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
    let got = worst_case(&[(&a, &[]), (&b, &[])], &params()).unwrap();
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
    let got = worst_case(&[(&a, &[])], &params()).unwrap();
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

fn steps() -> Vec<StepInput> {
    let mut out = Vec::new();
    for angle in [0.0, 90.0] {
        for speed in (2..=10).map(|i| f64::from(i) * 10.0) {
            let t: Vec<f64> = (0..3200).map(|i| f64::from(i) / 3200.0).collect();
            let wave = |f: f64, k: f64| -> Vec<f64> {
                t.iter()
                    .map(|t| (t * f * k).sin() * speed / 50.0 + (t * 977.0).cos() * 0.1)
                    .collect()
            };
            out.push(StepInput {
                angle,
                speed,
                samples: RawSamples {
                    x: wave(90.0 + angle, std::f64::consts::TAU),
                    y: wave(150.0 + speed, std::f64::consts::TAU),
                    z: wave(40.0, std::f64::consts::TAU),
                    t,
                },
            });
        }
    }
    out
}

#[test]
fn worst_case_of_one_chip_matches_analyze_chip_in_every_other_field() {
    let steps = steps();
    let mut want = analyze_chip(&steps, &params()).unwrap();
    // The grid is the one analyze_chip derived its figures from.
    let grid = Grid {
        angles: want.angles.clone(),
        speeds: want.speeds.clone(),
        data: want.grid.clone(),
    };
    let mut got = worst_case(&[(&grid, &want.measured)], &params()).unwrap();
    want.steps.clear();
    want.motor = None;
    got.steps.clear();
    got.motor = None;
    assert_eq!(got, want);
}

fn curve(angle: f64, speeds: &[f64], energy: &[f64]) -> MeasuredCurve {
    MeasuredCurve {
        angle,
        speeds: speeds.to_vec(),
        energy: energy.to_vec(),
    }
}

#[test]
fn combined_measured_is_the_intersection() {
    let (a, b) = (grid(1.0), grid(2.5));
    let speeds = [20.0, 30.0];
    let first = [
        curve(0.0, &speeds, &[1.0, 5.0]),
        curve(22.5, &speeds, &[2.0, 2.0]),
        curve(90.0, &speeds, &[3.0, 3.0]),
    ];
    let second = [
        curve(0.0, &speeds, &[4.0, 2.0]),
        curve(90.0, &speeds, &[1.0, 1.0]),
        curve(135.0, &speeds, &[9.0, 9.0]),
    ];
    let got = worst_case(&[(&a, &first), (&b, &second)], &params()).unwrap();
    assert_eq!(
        got.measured,
        vec![
            curve(0.0, &speeds, &[4.0, 5.0]),
            curve(90.0, &speeds, &[3.0, 3.0])
        ]
    );
}

#[test]
fn equal_axes_but_different_interior_speeds_give_no_measured_angle() {
    let (a, b) = (grid(1.0), grid(2.5));
    let first = [curve(0.0, &[20.0, 30.0, 40.0], &[1.0, 2.0, 3.0])];
    let second = [curve(0.0, &[20.0, 35.0, 40.0], &[1.0, 2.0, 3.0])];
    let got = worst_case(&[(&a, &first), (&b, &second)], &params()).unwrap();
    assert!(got.measured.is_empty());
}

#[test]
fn different_axes_give_no_combined_result() {
    let a = grid(1.0);
    let mut b = grid(1.0);
    b.speeds[3] += 1.0;
    let measured = [curve(0.0, &[20.0, 30.0], &[1.0, 2.0])];
    assert!(worst_case(&[(&a, &measured), (&b, &measured)], &params()).is_none());
}
