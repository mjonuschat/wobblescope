use std::f64::consts::TAU;

use wobblescope_core::RawSamples;
use wobblescope_core::vibrations::{
    EnergyByAngleSpeed, Kinematics, Model, StepInput, VibrationsParams, VibrationsResult,
    analyze_chip, angle_powers, direction_speed_grid, speed_powers, step_psds, symmetry,
};

const SPEEDS: [f64; 5] = [20.0, 30.0, 40.0, 50.0, 60.0];
const MAIN: [f64; 2] = [0.0, 90.0];
const MESSAGE: &str = "no usable vibration energy";

fn params() -> VibrationsParams {
    VibrationsParams {
        max_freq: 1000.0,
        kinematics: Kinematics::Cartesian,
    }
}

/// A one-second step whose energy grows with the square of `amplitude`.
fn step(angle: f64, speed: f64, amplitude: f64) -> StepInput {
    let t: Vec<f64> = (0..3200).map(|i| f64::from(i) / 3200.0).collect();
    let wave = |frequency: f64, level: f64| -> Vec<f64> {
        t.iter()
            .map(|t| (t * frequency * TAU).sin() * level)
            .collect()
    };
    StepInput {
        angle,
        speed,
        samples: RawSamples {
            x: wave(90.0, amplitude),
            y: wave(150.0, amplitude * 0.5),
            z: wave(40.0, 0.01),
            t,
        },
    }
}

fn steps(angles: &[f64], amplitude: impl Fn(f64, f64) -> f64) -> Vec<StepInput> {
    angles
        .iter()
        .flat_map(|angle| {
            SPEEDS
                .iter()
                .map(|speed| (*angle, *speed))
                .collect::<Vec<_>>()
        })
        .map(|(angle, speed)| step(angle, speed, amplitude(angle, speed)))
        .collect()
}

fn energy_of(steps: &[StepInput]) -> EnergyByAngleSpeed {
    let (_, psds) = step_psds(steps, 1000.0).unwrap();
    let mut energy: EnergyByAngleSpeed = Vec::new();
    for step in &psds {
        match energy.iter_mut().find(|(a, _)| *a == step.angle) {
            Some((_, by_speed)) => by_speed.push((step.speed, step.energy)),
            None => energy.push((step.angle, vec![(step.speed, step.energy)])),
        }
    }
    energy
}

fn ramps(angle: f64, speed: f64) -> f64 {
    if angle == 0.0 {
        speed / 10.0
    } else {
        (70.0 - speed) / 10.0
    }
}

#[test]
fn motor_only_output_is_unchanged() {
    let motor = steps(&MAIN, ramps);
    let got = analyze_chip(&motor, &params()).unwrap();
    let grid = direction_speed_grid(&energy_of(&motor), Kinematics::Cartesian, MAIN);
    let curves = speed_powers(&grid);
    assert_eq!(got.grid, grid.data);
    assert_eq!(got.speeds, grid.speeds);
    assert_eq!(got.angles, grid.angles);
    assert_eq!(got.angle_energy, angle_powers(&grid));
    assert_eq!(got.speed_min, curves.min);
    assert_eq!(got.speed_max, curves.max);
    assert_eq!(got.speed_variance, curves.variance);
    assert_eq!(got.vibration_metric, curves.metric);
    let want = symmetry(&grid, MAIN);
    assert_eq!(got.symmetry.is_some(), !want.is_nan());
}

#[test]
fn the_motor_profile_does_not_change_with_extras() {
    let motor = steps(&MAIN, |_, speed| speed / 10.0);
    let mut with_extras = motor.clone();
    with_extras.extend(steps(&[22.5, 45.0], |angle, speed| {
        speed / 10.0 + angle / 5.0
    }));
    let plain = analyze_chip(&motor, &params()).unwrap();
    let extended = analyze_chip(&with_extras, &params()).unwrap();
    assert!(plain.motor.is_some());
    assert_eq!(plain.motor, extended.motor);
}

#[test]
fn extras_change_the_good_angles() {
    let motor = steps(&MAIN, |_, speed| speed / 10.0);
    let mut with_extras = motor.clone();
    with_extras.extend(steps(&[45.0], |_, speed| speed));
    let plain = analyze_chip(&motor, &params()).unwrap();
    let extended = analyze_chip(&with_extras, &params()).unwrap();
    assert_ne!(plain.angle_energy, extended.angle_energy);
    assert_ne!(plain.good_angles, extended.good_angles);
    assert_eq!(
        plain.speeds, extended.speeds,
        "the extras never change the speed axis"
    );
}

#[test]
fn measured_lists_every_angle_with_steps() {
    let mut all = steps(&[90.0], |_, speed| speed / 10.0);
    all.extend(steps(&[22.5, 0.0], |_, speed| speed / 5.0));
    let got = analyze_chip(&all, &params()).unwrap();
    let angles: Vec<f64> = got.measured.iter().map(|m| m.angle).collect();
    assert_eq!(angles, vec![0.0, 22.5, 90.0]);
    for curve in &got.measured {
        assert_eq!(curve.speeds, SPEEDS.to_vec());
        for (speed, energy) in curve.speeds.iter().zip(&curve.energy) {
            let recorded = got
                .steps
                .iter()
                .find(|s| s.angle == curve.angle && s.speed == *speed)
                .unwrap();
            assert_eq!(recorded.energy, *energy);
        }
    }
}

#[test]
fn zero_variance_corrected_grid_fails_with_the_message() {
    let all = steps(&[0.0, 90.0, 45.0], |_, _| 1.0);
    let err = analyze_chip(&all, &params()).unwrap_err();
    assert!(err.to_string().contains(MESSAGE), "{err}");
}

#[test]
fn an_overflowing_corrected_grid_fails_the_chip() {
    let all = steps(&[0.0, 90.0, 45.0], |angle, speed| {
        if angle == 45.0 {
            1e100
        } else {
            speed / 10.0 * 1e100
        }
    });
    let err = analyze_chip(&all, &params()).unwrap_err();
    assert!(err.to_string().contains(MESSAGE), "{err}");
}

/// The amplitude whose step has `target` energy, read off the energy of two probe steps.
fn amplitude_for(target: f64) -> f64 {
    let energy = |amplitude: f64| energy_of(&[step(0.0, 20.0, amplitude)])[0].1[0].1;
    let (one, two) = (energy(1.0), energy(2.0));
    let quadratic = (two - one) / 3.0;
    ((target - (one - quadratic)) / quadratic).max(0.0).sqrt()
}

#[test]
fn a_corrected_grid_with_nan_symmetry_succeeds_with_symmetry_none() {
    let motor = steps(&MAIN, ramps);
    let motor_energy = energy_of(&motor);
    let plain = direction_speed_grid(&motor_energy, Kinematics::Cartesian, MAIN);
    assert!(
        symmetry(&plain, MAIN).is_nan(),
        "the fixture needs a NaN symmetry"
    );
    // Extras that measure what the model predicts leave the shape of the grid alone.
    let model = Model::new(&motor_energy, Kinematics::Cartesian, MAIN);
    let mut all = motor;
    all.extend(steps(&[22.5, 67.5], |angle, speed| {
        amplitude_for(model.value(angle, speed))
    }));
    let got: VibrationsResult = analyze_chip(&all, &params()).unwrap();
    assert_eq!(got.symmetry, None);
    assert_eq!(got.grid.len(), 720);
}

#[test]
fn a_motor_only_degenerate_input_keeps_todays_result() {
    let flat = steps(&MAIN, |_, _| 1.0);
    let got = analyze_chip(&flat, &params()).unwrap();
    let grid = direction_speed_grid(&energy_of(&flat), Kinematics::Cartesian, MAIN);
    let curves = speed_powers(&grid);
    assert_eq!(
        format!("{:?}", got.speed_variance),
        format!("{:?}", curves.variance)
    );
    assert_eq!(
        format!("{:?}", got.vibration_metric),
        format!("{:?}", curves.metric)
    );
}
