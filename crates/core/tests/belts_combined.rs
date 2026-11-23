//! The 5 Hz floor and the tolerance multiplier together, on the path a session takes.
//! This file exists only because both changes do, so reverting either one removes it.
#[macro_use]
mod common;

use common::belt_fixtures;
use wobblescope_core::belts::{
    Belt, ImbalanceState, TOLERANCE, compare_psds, compare_psds_in_band,
};

parity_test! {
    fn the_unequal_capture_is_found_and_not_excellent_under_the_floor() {
        // (chip, max_freq, figure, lo_hz, hi_hz, pairs, health), observed with the 5 Hz band.
        let observed = [
            ("adxl345 carto", 150.0, 0.32383, 115.0, 145.0, 1, "Potential signs of a mechanical issue"),
            ("adxl345 carto", 200.0, 0.31199, 165.0, 195.0, 1, "Potential signs of a mechanical issue"),
            ("adxl345 nh36", 150.0, 0.48073, 115.0, 145.0, 2, "Good mechanical health"),
            ("adxl345 nh36", 200.0, 0.39794, 155.0, 185.0, 2, "Acceptable mechanical health"),
        ];
        let fixtures = belt_fixtures("belts_unequal_power");
        for (chip, max_freq, figure, lo, hi, pairs, health) in observed {
            let f = fixtures.iter().find(|f| f.name == chip).expect(chip);
            let got = compare_psds(&f.freqs_a, &f.psd_a, &f.freqs_b, &f.psd_b, max_freq);
            let label = format!("{chip} at {max_freq}");
            assert_eq!(got.imbalance.state, ImbalanceState::Found, "{label}");
            assert_eq!(got.imbalance.stronger, Some(Belt::B), "{label}");
            assert!((got.imbalance.figure - figure).abs() < 1e-4, "{label}: {:?}", got.imbalance);
            assert_eq!((got.imbalance.lo_hz, got.imbalance.hi_hz), (lo, hi), "{label}");
            assert_eq!(got.pairs.len(), pairs, "{label}");
            assert_ne!(got.health, "Excellent mechanical health", "{label}");
            assert_eq!(got.health, health, "{label}");
        }
    }
}

parity_test! {
    fn the_balanced_capture_keeps_its_baseline_under_the_floor_and_with_its_sweep() {
        // The capture was swept 5 to 135 Hz. Up to 150 Hz the floor reads nearly the same band;
        // at 200 Hz it would read 135 to 200 Hz of noise that the test never excited, so only
        // the recorded sweep is asserted there.
        let fixtures = belt_fixtures("belts_balanced");
        let f = fixtures.iter().find(|f| f.name == "lis2dw ebb36").expect("lis2dw ebb36");
        assert!(f.sweep.is_some());
        for (max_freq, pairs, health) in [
            (150.0, 3, "Good mechanical health"),
            (200.0, 3, "Good mechanical health"),
        ] {
            let floored = compare_psds(&f.freqs_a, &f.psd_a, &f.freqs_b, &f.psd_b, max_freq);
            let swept = compare_psds_in_band(&f.freqs_a, &f.psd_a, &f.freqs_b, &f.psd_b, max_freq, f.sweep);
            let mut paths = vec![("sweep", swept)];
            if max_freq == 150.0 {
                paths.push(("floor", floored));
            }
            for (path, got) in paths {
                let label = format!("{path} at {max_freq}");
                assert!(got.imbalance.figure >= TOLERANCE, "{label}: {:?}", got.imbalance);
                assert_eq!(got.pairs.len(), pairs, "{label}");
                assert_eq!(got.health, health, "{label}");
            }
        }
    }
}
