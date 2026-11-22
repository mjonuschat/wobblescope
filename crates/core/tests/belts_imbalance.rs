#[macro_use]
mod common;

use common::belt_fixtures;
use wobblescope_core::belts::{Belt, BeltsResult, ImbalanceState, TOLERANCE, compare_psds_in_band};

fn compare(file: &str, name: &str, max_freq: f64) -> BeltsResult {
    let fixtures = belt_fixtures(file);
    let f = fixtures
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no chip {name} in {file}"));
    compare_psds_in_band(
        &f.freqs_a, &f.psd_a, &f.freqs_b, &f.psd_b, max_freq, f.sweep,
    )
}

/// The observed unequal-capture results, with the baseline pair count unchanged by
/// the imbalance check.
struct Unequal {
    chip: &'static str,
    max_freq: f64,
    figure: f64,
    multiplier: f64,
    lo_hz: f64,
    hi_hz: f64,
    pairs: usize,
    health: &'static str,
}

parity_test! {
    fn the_unequal_capture_is_found_and_not_excellent_on_either_chip() {
        // The multiplier is figure / 0.8. The range moves with the comparison limit: the
        // lowest-median run is searched over the whole band up to it.
        let observed = [
            Unequal { chip: "adxl345 carto", max_freq: 150.0, figure: 0.34764, multiplier: 0.43455,
                lo_hz: 120.0, hi_hz: 150.0, pairs: 1, health: "Potential signs of a mechanical issue" },
            Unequal { chip: "adxl345 carto", max_freq: 200.0, figure: 0.34409, multiplier: 0.43011,
                lo_hz: 170.0, hi_hz: 200.0, pairs: 1, health: "Potential signs of a mechanical issue" },
            Unequal { chip: "adxl345 nh36", max_freq: 150.0, figure: 0.43278, multiplier: 0.54097,
                lo_hz: 120.0, hi_hz: 150.0, pairs: 2, health: "Acceptable mechanical health" },
            Unequal { chip: "adxl345 nh36", max_freq: 200.0, figure: 0.40130, multiplier: 0.50162,
                lo_hz: 160.0, hi_hz: 190.0, pairs: 2, health: "Potential signs of a mechanical issue" },
        ];
        for case in observed {
            let got = compare("belts_unequal_power", case.chip, case.max_freq);
            let label = format!("{} at {}", case.chip, case.max_freq);
            assert_eq!(got.imbalance.state, ImbalanceState::Found, "{label}");
            assert_eq!(got.imbalance.stronger, Some(Belt::B), "{label}");
            assert!((got.imbalance.figure - case.figure).abs() < 1e-4, "{label}: {:?}", got.imbalance);
            assert!((got.imbalance.figure / TOLERANCE - case.multiplier).abs() < 1e-4, "{label}");
            assert_eq!((got.imbalance.lo_hz, got.imbalance.hi_hz), (case.lo_hz, case.hi_hz), "{label}");
            assert_eq!(got.pairs.len(), case.pairs, "{label}");
            assert_ne!(got.health, "Excellent mechanical health", "{label}");
            assert_eq!(got.health, case.health, "{label}");
        }
    }
}

parity_test! {
    fn the_balanced_capture_keeps_its_baseline() {
        for (max_freq, pairs, health) in [
            (150.0, 4, "Potential signs of a mechanical issue"),
            (200.0, 3, "Good mechanical health"),
        ] {
            let got = compare("belts_balanced", "lis2dw ebb36", max_freq);
            let label = format!("at {max_freq}");
            assert!(got.imbalance.figure >= TOLERANCE, "{label}: {:?}", got.imbalance);
            assert_eq!(got.pairs.len(), pairs, "{label}");
            assert_eq!(got.health, health, "{label}");
        }
    }
}
