//! Parity of the vibrations profile with Shake&Tune's `VibrationsComputation`, on the fixtures
//! written by `tests/golden/vibrations_regen.py`.

#[macro_use]
mod common;

use common::{Fixture, assert_close, close, fixtures, inputs};
use wobblescope_core::RawSamples;
use wobblescope_core::peaks::detect_peaks_relative;
use wobblescope_core::vibrations::{
    EnergyByAngleSpeed, Kinematics, PsdByAngleSpeed, StepInput, VibrationsParams, Zone,
    analyze_chip, angle_powers, direction_speed_grid, estimate_damping_above,
    filter_and_split_zones, low_energy_zones, motor_profiles, speed_powers, step_psds, symmetry,
};

parity_test! {
    fn step_energy_matches_shaketune() {
        for (name, fixture) in fixtures() {
            let (freqs, steps) = step_psds(&inputs(&fixture), fixture.max_freq).unwrap();
            assert_close(&format!("{name} target freqs"), &freqs, &fixture.target_freqs);
            assert_eq!(steps.len(), fixture.psds.len(), "{name}: one entry per step");
            for step in &steps {
                let key = format!("{name} angle {} speed {}", step.angle, step.speed);
                let psd = fixture
                    .psds
                    .iter()
                    .find(|p| p.angle == step.angle && p.speed == step.speed)
                    .unwrap_or_else(|| panic!("{key}: no PSD in the fixture"));
                let energy = fixture
                    .energies
                    .iter()
                    .find(|e| e.angle == step.angle && e.speed == step.speed)
                    .unwrap_or_else(|| panic!("{key}: no energy in the fixture"));
                assert_close(&format!("{key} psd"), &step.psd, &psd.psd);
                close(&format!("{key} energy"), step.energy, energy.energy);
            }
        }
    }
}

parity_test! {
    fn duplicate_speed_keeps_the_later_step() {
        let (_, fixture) = fixtures().remove(0);
        let mut steps = inputs(&fixture);
        let first = steps[0].clone();
        let mut later = first.clone();
        later.samples.x.iter_mut().for_each(|v| *v *= 2.0);
        let mut doubled = vec![later.clone()];
        doubled.extend(steps.iter().skip(1).cloned());
        let (_, want) = step_psds(&doubled, fixture.max_freq).unwrap();

        steps.push(later);
        let (_, got) = step_psds(&steps, fixture.max_freq).unwrap();

        assert_eq!(got.len(), fixture.steps.len(), "the duplicate adds no entry");
        assert_eq!(got[0].angle, first.angle);
        assert_eq!(got[0].speed, first.speed);
        assert_close("replaced psd", &got[0].psd, &want[0].psd);
        close("replaced energy", got[0].energy, want[0].energy);
    }
}

fn main_angles(fixture: &Fixture) -> [f64; 2] {
    [fixture.main_angles[0], fixture.main_angles[1]]
}

fn energy_by_angle(fixture: &Fixture) -> EnergyByAngleSpeed {
    let mut out: EnergyByAngleSpeed = Vec::new();
    for e in &fixture.energies {
        match out.iter_mut().find(|(a, _)| *a == e.angle) {
            Some((_, by_speed)) => by_speed.push((e.speed, e.energy)),
            None => out.push((e.angle, vec![(e.speed, e.energy)])),
        }
    }
    out
}

fn psd_by_angle(fixture: &Fixture) -> PsdByAngleSpeed {
    let mut out: PsdByAngleSpeed = Vec::new();
    for p in &fixture.psds {
        match out.iter_mut().find(|(a, _)| *a == p.angle) {
            Some((_, by_speed)) => by_speed.push((p.speed, p.psd.clone())),
            None => out.push((p.angle, vec![(p.speed, p.psd.clone())])),
        }
    }
    out
}

fn grid(fixture: &Fixture) -> wobblescope_core::vibrations::Grid {
    let k = Kinematics::parse(&fixture.kinematics).unwrap();
    direction_speed_grid(&energy_by_angle(fixture), k, main_angles(fixture))
}

fn assert_zones(label: &str, got: &[Zone], want: &[(usize, usize, f64)]) {
    assert_eq!(got.len(), want.len(), "{label}: number of zones");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert_eq!((g.start, g.end), (w.0, w.1), "{label}[{i}] bounds");
        close(&format!("{label}[{i}] energy"), g.energy, w.2);
    }
}

parity_test! {
    fn grid_matches_shaketune() {
        for (name, fixture) in fixtures() {
            let got = grid(&fixture);
            assert_close(&format!("{name} angles"), &got.angles, &fixture.all_angles);
            assert_close(&format!("{name} speeds"), &got.speeds, &fixture.all_speeds);
            assert_eq!(got.data.len(), fixture.spectrogram_data.len());
            for (i, (g, w)) in got.data.iter().zip(&fixture.spectrogram_data).enumerate() {
                assert_close(&format!("{name} spectrogram row {i}"), g, w);
            }
        }
    }
}

parity_test! {
    fn angle_powers_match() {
        for (name, fixture) in fixtures() {
            let got = angle_powers(&grid(&fixture));
            assert_close(&format!("{name} angle powers"), &got, &fixture.all_angles_energy);
        }
    }
}

parity_test! {
    fn speed_curves_match() {
        for (name, fixture) in fixtures() {
            let got = speed_powers(&grid(&fixture));
            assert_close(&format!("{name} min"), &got.min, &fixture.sp_min_energy);
            assert_close(&format!("{name} max"), &got.max, &fixture.sp_max_energy);
            assert_close(&format!("{name} variance"), &got.variance, &fixture.sp_variance_energy);
            assert_close(&format!("{name} metric"), &got.metric, &fixture.vibration_metric);
        }
    }
}

parity_test! {
    fn peaks_and_good_speeds_match() {
        for (name, fixture) in fixtures() {
            let metric = &fixture.vibration_metric;
            let max = metric.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let peaks = detect_peaks_relative(metric, 0.05 * max, 0.04, 10, 10);
            assert_eq!(peaks, fixture.vibration_peaks, "{name} peaks");

            let speeds = &fixture.all_speeds;
            let peak_indices: Vec<(f64, usize)> = peaks.iter().map(|i| (speeds[*i], *i)).collect();
            let deletion_range = (3.0 / (speeds[1] - speeds[0])) as usize;
            let zones = low_energy_zones(metric, 0.7);
            let got = filter_and_split_zones(speeds, &zones, &peak_indices, deletion_range);
            assert_zones(&format!("{name} good speeds"), &got, &fixture.good_speeds);
        }
    }
}

parity_test! {
    fn good_angles_match() {
        for (name, fixture) in fixtures() {
            let got = low_energy_zones(&fixture.all_angles_energy, 1.1);
            assert_zones(&format!("{name} good angles"), &got, &fixture.good_angles);
        }
    }
}

parity_test! {
    fn motor_profile_weights_index_by_degrees() {
        for (name, fixture) in fixtures() {
            let main = main_angles(&fixture);
            let (profiles, global) = motor_profiles(
                &fixture.target_freqs,
                &psd_by_angle(&fixture),
                &fixture.all_angles_energy,
                main,
            );
            assert_eq!(profiles.len(), 2);
            for (angle, profile) in &profiles {
                let want = &fixture.motor_profiles[&format!("{}", *angle as i64)];
                assert_close(&format!("{name} profile {angle}"), profile, want);
            }
            assert_close(&format!("{name} global profile"), &global, &fixture.global_motor_profile);

            // The weight of 90 degrees reads row 90 (45 degrees), not row 180. Moving the
            // energy of that row changes the result, moving the energy of row 180 does not.
            let mut energy = fixture.all_angles_energy.clone();
            let row = main[1] as usize;
            let nearest_row = 2 * row;
            energy[nearest_row] *= 10.0;
            let (_, unchanged) =
                motor_profiles(&fixture.target_freqs, &psd_by_angle(&fixture), &energy, main);
            assert_close(&format!("{name} row {nearest_row} unused"), &unchanged, &global);
            let mut energy = fixture.all_angles_energy.clone();
            energy[row] *= 10.0;
            let (_, changed) =
                motor_profiles(&fixture.target_freqs, &psd_by_angle(&fixture), &energy, main);
            assert!(
                changed.iter().zip(&global).any(|(c, g)| (c - g).abs() > 1e-6 * g.abs()),
                "{name}: row {row} must carry the weight"
            );
        }
    }
}

parity_test! {
    fn mechanical_parameters_match() {
        for (name, fixture) in fixtures() {
            let got = estimate_damping_above(&fixture.target_freqs, &fixture.global_motor_profile, 30.0)
                .unwrap();
            close(&format!("{name} fr"), got.fr, fixture.motor_fr);
            assert_eq!(got.peak_index, fixture.motor_res_idx, "{name} peak index");
            assert_eq!(got.low_freq_max, fixture.motor_lowfreq_max, "{name} low_freq_max");
            match (got.zeta, fixture.motor_zeta) {
                (Some(g), Some(w)) => close(&format!("{name} zeta"), g, w),
                (g, w) => assert_eq!(g, w, "{name} zeta"),
            }
        }
    }
}

parity_test! {
    fn low_freq_max_is_set_when_the_strongest_bin_is_below_min_freq() {
        let freqs: Vec<f64> = (0..100).map(f64::from).collect();
        let mut psd = vec![1.0; 100];
        psd[10] = 9.0;
        psd[60] = 5.0;
        let got = estimate_damping_above(&freqs, &psd, 30.0).unwrap();
        assert!(got.low_freq_max);
        assert_eq!((got.peak_index, got.fr), (60, 60.0));
        assert!(estimate_damping_above(&freqs, &psd, 500.0).is_none());
    }
}

parity_test! {
    fn symmetry_matches() {
        for (name, fixture) in fixtures() {
            let got = symmetry(&grid(&fixture), main_angles(&fixture));
            close(&format!("{name} symmetry"), got, fixture.symmetry_factor);
        }
    }
}

fn params(fixture: &Fixture) -> VibrationsParams {
    VibrationsParams {
        max_freq: fixture.max_freq,
        kinematics: Kinematics::parse(&fixture.kinematics).unwrap(),
    }
}

parity_test! {
    fn analyze_chip_matches_shaketune_end_to_end() {
        for (name, fixture) in fixtures() {
            let got = analyze_chip(&inputs(&fixture), &params(&fixture)).unwrap();
            assert_eq!(got.steps.len(), fixture.energies.len(), "{name} steps");
            for step in &got.steps {
                let want = fixture
                    .energies
                    .iter()
                    .find(|e| e.angle == step.angle && e.speed == step.speed)
                    .unwrap_or_else(|| panic!("{name}: step {} {} missing", step.angle, step.speed));
                close(&format!("{name} step energy"), step.energy, want.energy);
            }
            assert_close(&format!("{name} speeds"), &got.speeds, &fixture.all_speeds);
            assert_close(&format!("{name} angles"), &got.angles, &fixture.all_angles);
            assert_close(&format!("{name} angle energy"), &got.angle_energy, &fixture.all_angles_energy);
            assert_eq!(got.grid.len(), fixture.spectrogram_data.len());
            for (i, (g, w)) in got.grid.iter().zip(&fixture.spectrogram_data).enumerate() {
                assert_close(&format!("{name} grid row {i}"), g, w);
            }
            assert_close(&format!("{name} min"), &got.speed_min, &fixture.sp_min_energy);
            assert_close(&format!("{name} max"), &got.speed_max, &fixture.sp_max_energy);
            assert_close(&format!("{name} variance"), &got.speed_variance, &fixture.sp_variance_energy);
            assert_close(&format!("{name} metric"), &got.vibration_metric, &fixture.vibration_metric);
            assert_eq!(got.peaks, fixture.vibration_peaks, "{name} peaks");
            let peak_speeds: Vec<f64> = fixture.vibration_peaks.iter().map(|i| fixture.all_speeds[*i]).collect();
            assert_close(&format!("{name} peak speeds"), &got.peak_speeds, &peak_speeds);
            assert_zones(&format!("{name} good speeds"), &got.good_speeds, &fixture.good_speeds);
            // The two mirror-image zones tie to within rounding, so which comes first depends on
            // the last bit of the step energies. Compare the zones in a fixed order.
            let mut got_angles = got.good_angles.clone();
            got_angles.sort_by_key(|z| z.start);
            let mut want_angles = fixture.good_angles.clone();
            want_angles.sort_by_key(|z| z.0);
            assert_zones(&format!("{name} good angles"), &got_angles, &want_angles);
            close(&format!("{name} symmetry"), got.symmetry.unwrap(), fixture.symmetry_factor);
            let motor = got.motor.expect("motor");
            assert_close(&format!("{name} motor freqs"), &motor.freqs, &fixture.target_freqs);
            assert_eq!(motor.profiles.len(), 2);
            for (angle, profile) in &motor.profiles {
                let want = &fixture.motor_profiles[&format!("{}", *angle as i64)];
                assert_close(&format!("{name} profile {angle}"), profile, want);
            }
            assert_close(&format!("{name} combined"), &motor.combined, &fixture.global_motor_profile);
            close(&format!("{name} fr"), motor.fr, fixture.motor_fr);
            assert_eq!(motor.peak_index, fixture.motor_res_idx);
            assert_eq!(motor.low_freq_warning, fixture.motor_lowfreq_max);
            match (motor.zeta, fixture.motor_zeta) {
                (Some(g), Some(w)) => close(&format!("{name} zeta"), g, w),
                (g, w) => assert_eq!(g, w, "{name} zeta"),
            }
            assert_eq!(got.core_version, wobblescope_core::CORE_VERSION);
        }
    }
}

parity_test! {
    fn a_flat_recording_is_an_error_not_nan() {
        let (_, fixture) = fixtures().remove(0);
        let mut steps = inputs(&fixture);
        steps[3].samples = RawSamples { t: vec![], x: vec![], y: vec![], z: vec![] };
        assert!(analyze_chip(&steps, &params(&fixture)).is_err());
    }
}

parity_test! {
    fn a_missing_main_angle_is_an_error() {
        let (_, fixture) = fixtures().remove(0);
        let main = main_angles(&fixture)[1];
        let steps: Vec<StepInput> = inputs(&fixture).into_iter().filter(|s| s.angle != main).collect();
        assert!(analyze_chip(&steps, &params(&fixture)).is_err());
    }
}

parity_test! {
    fn a_single_speed_is_an_error() {
        let (_, fixture) = fixtures().remove(0);
        let first = inputs(&fixture)[0].speed;
        let steps: Vec<StepInput> = inputs(&fixture).into_iter().filter(|s| s.speed == first).collect();
        assert!(steps.len() >= 2, "both main angles at one speed");
        assert!(analyze_chip(&steps, &params(&fixture)).is_err());
    }
}
