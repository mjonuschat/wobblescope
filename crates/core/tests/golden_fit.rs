#[macro_use]
mod common;

use common::{
    Case, REAL_CASES, assert_close, assert_result, case, check_recommendation, close, fit_params,
    normalized, stem,
};
use wobblescope_core::fit::{Axis, FitParams, Recommendation, find_best_shaper, fit_shaper};
use wobblescope_core::shaper::{AUTOTUNE_SHAPERS, builtin};
use wobblescope_core::{CORE_VERSION, CoreError};

fn recommend(case_name: &str) -> (Recommendation, Case) {
    let case = case(case_name);
    let data: Vec<_> = case.inputs.iter().map(|i| normalized(stem(i))).collect();
    let rec = find_best_shaper(&data, &fit_params(&case.params)).unwrap();
    (rec, case)
}

parity_test! {
    fn candidates_match_klipper_for_chip_a_and_chip_b() {
        for name in ["chip_a", "chip_b"].into_iter().chain(REAL_CASES) {
            let case = case(name);
            let data: Vec<_> = case.inputs.iter().map(|i| normalized(stem(i))).collect();
            for (shaper_name, entry) in &case.fit {
                let shaper = builtin(shaper_name).unwrap();
                let got = fit_shaper(shaper, &data, &fit_params(&case.params)).unwrap();
                assert_eq!(got.len(), entry.candidates.len() + 1, "{name} {shaper_name}");
                assert_result(&format!("{name} {shaper_name} selected"), &got[0], &entry.selected);
                for (i, (g, w)) in got[1..].iter().zip(&entry.candidates).enumerate() {
                    assert_result(&format!("{name} {shaper_name} candidate {i}"), g, w);
                }
            }
        }
    }
}

parity_test! {
    fn final_shaper_values_match_klipper() {
        for name in ["chip_a", "chip_b"].into_iter().chain(REAL_CASES) {
            let (rec, case) = recommend(name);
            for got in &rec.all {
                let want = &case.fit[&got.name].fin;
                let label = format!("{name} {}", got.name);
                close(&format!("{label} freq"), got.freq, want.freq);
                close(&format!("{label} score"), got.score, want.score);
                assert_close(&format!("{label} freq_bins"), &got.freq_bins, &want.freq_bins);
                assert_close(&format!("{label} vals"), &got.vals, &want.vals);
            }
        }
    }
}

parity_test! {
    fn recommendation_matches_klipper_or_logs_near_tie() {
        for name in ["chip_a", "chip_b"].into_iter().chain(REAL_CASES) {
            let (rec, case) = recommend(name);
            check_recommendation(name, &rec, &case);
        }
    }
}

parity_test! {
    fn parameter_variants_match_klipper() {
        for name in ["chip_a.max_smoothing", "chip_a.scv10_damp05", "chip_a.max_vibrs"] {
            let (rec, case) = recommend(name);
            check_recommendation(name, &rec, &case);
            for got in &rec.all {
                let want = &case.fit[&got.name].fin;
                assert_close(&format!("{name} {} vals", got.name), &got.vals, &want.vals);
            }
        }
    }
}

parity_test! {
    fn recommendation_records_settings_and_version() {
        let (rec, _) = recommend("chip_a");
        assert_eq!(rec.settings.scv, 5.0);
        assert_eq!(rec.settings.freq_step, 0.2);
        assert_eq!(rec.settings.shapers, AUTOTUNE_SHAPERS);
        assert_eq!(rec.core_version, CORE_VERSION);
    }
}

parity_test! {
    fn config_snippet_formats_axis_one_decimal_and_damping() {
        let (mut rec, _) = recommend("chip_a");
        rec.best.name = "mzv".to_owned();
        rec.best.freq = 53.449;
        assert_eq!(
            rec.config_snippet(Axis::X),
            "[input_shaper]\nshaper_type_x: mzv\nshaper_freq_x: 53.4\ndamping_ratio_x: 0.1\n"
        );
        assert!(rec.config_snippet(Axis::Y).contains("shaper_freq_y: 53.4\n"));
    }
}

parity_test! {
    fn analysis_types_round_trip_through_json() {
        let (rec, _) = recommend("chip_b");
        let json = serde_json::to_string(&rec).unwrap();
        let back: Recommendation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rec);
    }
}

parity_test! {
    fn unknown_shaper_name_in_params_is_an_error() {
        let params = FitParams { shapers: Some(vec!["bogus".to_owned()]), ..FitParams::default() };
        assert_eq!(
            find_best_shaper(&[normalized("chip_b")], &params),
            Err(CoreError::UnknownShaper("bogus".to_owned()))
        );
    }
}

parity_test! {
    fn a_dataset_that_is_not_normalised_is_normalised_before_the_fit() {
        let raw = wobblescope_core::CalibrationData::from_samples("chip_b", &common::load_csv("chip_b")).unwrap();
        assert!(!raw.is_normalized());
        let params = FitParams::default();
        let from_raw = find_best_shaper(std::slice::from_ref(&raw), &params).unwrap();
        let from_normalised = find_best_shaper(&[normalized("chip_b")], &params).unwrap();
        assert_eq!(from_raw, from_normalised);
        let shaper = builtin("mzv").unwrap();
        assert_eq!(
            fit_shaper(shaper, &[raw], &params).unwrap(),
            fit_shaper(shaper, &[normalized("chip_b")], &params).unwrap()
        );
    }
}

parity_test! {
    fn flat_psd_yields_flat_signal_error() {
        let mut data = normalized("chip_b");
        data.psd_sum = vec![0.0; data.psd_sum.len()];
        assert_eq!(
            find_best_shaper(&[data], &FitParams::default()),
            Err(CoreError::FlatSignal)
        );
    }
}

// Native only: panics abort on wasm32, so `should_panic` cannot run there.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[should_panic(expected = "max_accel")]
fn recommendation_check_fails_when_max_accel_is_wrong() {
    let (mut rec, case) = recommend("chip_a");
    rec.all[0].max_accel *= 1.5;
    check_recommendation("mutated", &rec, &case);
}

// Native only: panics abort on wasm32, so `should_panic` cannot run there.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[should_panic(expected = "best")]
fn recommendation_check_fails_when_best_metrics_are_wrong() {
    let (mut rec, case) = recommend("chip_a");
    rec.best.max_accel *= 1.5;
    check_recommendation("mutated", &rec, &case);
}

/// `chip_a` with its recommendation replaced by the MZV candidate 0.2 Hz above Klipper's.
fn near_tie_recommendation() -> (Recommendation, Case) {
    let (mut rec, case) = recommend("chip_a");
    let shaper = builtin(&case.best.name).unwrap();
    let cands = fit_shaper(shaper, &[normalized("chip_a")], &fit_params(&case.params)).unwrap();
    let alt = cands
        .into_iter()
        .skip(1)
        .find(|c| (c.freq - (case.best.freq + 0.2)).abs() < 1e-6)
        .expect("candidate 0.2 Hz above the best");
    rec.best = alt;
    (rec, case)
}

parity_test! {
    fn a_near_tie_candidate_is_accepted() {
        let (rec, case) = near_tie_recommendation();
        check_recommendation("near tie", &rec, &case);
    }
}

// Native only: panics abort on wasm32, so `should_panic` cannot run there.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[should_panic(expected = "max_accel")]
fn recommendation_check_fails_when_a_near_tie_has_the_wrong_max_accel() {
    let (mut rec, case) = near_tie_recommendation();
    rec.best.max_accel *= 1.5;
    check_recommendation("mutated near tie", &rec, &case);
}
