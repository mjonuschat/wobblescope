use std::f64::consts::TAU;

use wobblescope_core::RawSamples;
use wobblescope_core::numeric::trapz;
use wobblescope_core::vibrations::{
    Kinematics, PsdByAngleSpeed, StepInput, VibrationsParams, analyze_chip, motor_profiles,
    motor_profiles_by_angle,
};

const ROWS: usize = 720;

fn freqs() -> Vec<f64> {
    (0..=100).map(f64::from).collect()
}

fn bump(centre: f64, height: f64) -> Vec<f64> {
    freqs()
        .iter()
        .map(|f| height * (-((f - centre) / 8.0).powi(2)).exp())
        .collect()
}

fn psds() -> PsdByAngleSpeed {
    vec![
        (45.0, vec![(10.0, bump(40.0, 3.0)), (20.0, bump(40.0, 4.0))]),
        (
            135.0,
            vec![(10.0, bump(70.0, 2.0)), (20.0, bump(70.0, 1.0))],
        ),
    ]
}

fn row_of(angle: f64) -> usize {
    (angle / 360.0 * (ROWS - 1) as f64).round() as usize
}

fn energy_with(rows: &[(usize, f64)]) -> Vec<f64> {
    let mut energy = vec![1.0; ROWS];
    for (row, value) in rows {
        energy[*row] = *value;
    }
    energy
}

fn close(label: &str, got: &[f64], want: &[f64]) {
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(
            (g - w).abs() <= 1e-9 * w.abs().max(1e-12),
            "{label}[{i}]: {g} vs {w}"
        );
    }
}

fn weighted(profiles: &[(f64, Vec<f64>)], energy: [f64; 2]) -> Vec<f64> {
    let freqs = freqs();
    let weights: Vec<f64> = profiles
        .iter()
        .zip(energy)
        .map(|((_, p), e)| e.powi(2) * trapz(p, &freqs).powi(2))
        .collect();
    let total: f64 = weights.iter().sum();
    (0..freqs.len())
        .map(|i| {
            profiles
                .iter()
                .zip(&weights)
                .map(|((_, p), w)| p[i] * w)
                .sum::<f64>()
                / total
        })
        .collect()
}

#[test]
fn the_weight_of_an_angle_is_the_energy_of_the_row_nearest_it() {
    let main = [45.0, 135.0];
    let energy = energy_with(&[(row_of(45.0), 3.0), (row_of(135.0), 0.5)]);
    let (profiles, global) = motor_profiles_by_angle(&freqs(), &psds(), &energy, main);
    close("global", &global, &weighted(&profiles, [3.0, 0.5]));
}

#[test]
fn the_rows_the_degree_number_would_name_carry_no_weight() {
    let main = [45.0, 135.0];
    let base = energy_with(&[(row_of(45.0), 3.0), (row_of(135.0), 0.5)]);
    let (_, global) = motor_profiles_by_angle(&freqs(), &psds(), &base, main);
    // Row 45 is the angle 22.5 degrees, which Shake&Tune reads for 45.
    let mut moved = base.clone();
    moved[45] = 9.0;
    let (_, same) = motor_profiles_by_angle(&freqs(), &psds(), &moved, main);
    close("rows by degree number", &same, &global);
    let (_, reference_before) = motor_profiles(&freqs(), &psds(), &base, main);
    let (_, reference_after) = motor_profiles(&freqs(), &psds(), &moved, main);
    assert!(
        reference_before
            .iter()
            .zip(&reference_after)
            .any(|(a, b)| (a - b).abs() > 1e-6),
        "the reference reads those rows"
    );
}

#[test]
fn the_rows_for_zero_and_ninety_degrees_are_the_first_and_the_one_a_quarter_in() {
    let main = [0.0, 90.0];
    let psds: PsdByAngleSpeed = vec![
        (0.0, vec![(10.0, bump(40.0, 3.0))]),
        (90.0, vec![(10.0, bump(70.0, 2.0))]),
    ];
    let energy = energy_with(&[(0, 2.0), (180, 5.0)]);
    let (profiles, global) = motor_profiles_by_angle(&freqs(), &psds, &energy, main);
    close("global", &global, &weighted(&profiles, [2.0, 5.0]));
}

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

#[test]
fn an_analysis_weighs_the_motor_profile_by_the_energy_at_each_main_angle() {
    let steps: Vec<StepInput> = [0.0, 90.0]
        .into_iter()
        .flat_map(|angle| {
            [20.0, 30.0, 40.0, 50.0, 60.0].map(|speed| {
                let amplitude = if angle == 0.0 {
                    speed / 10.0
                } else {
                    (70.0 - speed) / 4.0
                };
                step(angle, speed, amplitude)
            })
        })
        .collect();
    let params = VibrationsParams {
        max_freq: 1000.0,
        kinematics: Kinematics::Cartesian,
    };
    let result = analyze_chip(&steps, &params).unwrap();
    let motor = result.motor.expect("a motor profile");
    let energy = [
        result.angle_energy[row_of(0.0)],
        result.angle_energy[row_of(90.0)],
    ];
    let want = {
        let weights: Vec<f64> = motor
            .profiles
            .iter()
            .zip(energy)
            .map(|((_, p), e)| e.powi(2) * trapz(p, &motor.freqs).powi(2))
            .collect();
        let total: f64 = weights.iter().sum();
        (0..motor.freqs.len())
            .map(|i| {
                motor
                    .profiles
                    .iter()
                    .zip(&weights)
                    .map(|((_, p), w)| p[i] * w)
                    .sum::<f64>()
                    / total
            })
            .collect::<Vec<_>>()
    };
    close("combined", &motor.combined, &want);
}
