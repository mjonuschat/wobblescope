#[macro_use]
mod common;

use common::{case, fit_params, normalized, shaketune, stem};
use wobblescope_core::fit::{CalibrationResult, find_best_shaper, performance_pick};

fn result(name: &str, vibrs: f64, max_accel: f64) -> CalibrationResult {
    CalibrationResult {
        name: name.to_owned(),
        freq: 50.0,
        vals: vec![],
        vibrs,
        dataset_vibrs: vec![vibrs],
        smoothing: 0.1,
        score: 1.0,
        max_accel,
    }
}

#[test]
fn picks_the_highest_max_accel_under_five_percent() {
    let all = [
        result("zv", 0.01, 3000.0),
        result("ei", 0.04, 6000.0),
        result("mzv", 0.09, 9000.0),
    ];
    let got = performance_pick(&all[0], &all).unwrap();
    assert_eq!(got.name, "ei");
}

#[test]
fn a_tie_goes_to_the_first() {
    let all = [
        result("zv", 0.01, 3000.0),
        result("mzv", 0.02, 6000.0),
        result("ei", 0.03, 6000.0),
    ];
    assert_eq!(performance_pick(&all[0], &all).unwrap().name, "mzv");
}

#[test]
fn none_when_the_pick_is_the_best() {
    let all = [result("zv", 0.01, 6000.0), result("ei", 0.04, 3000.0)];
    assert_eq!(performance_pick(&all[0], &all), None);
}

#[test]
fn none_when_its_accel_is_below_the_bests() {
    // The best has a higher acceleration than anything under five percent.
    let all = [result("mzv", 0.09, 9000.0), result("zv", 0.01, 3000.0)];
    assert_eq!(performance_pick(&all[0], &all), None);
}

#[test]
fn none_when_every_shaper_is_at_or_above_five_percent() {
    let all = [result("zv", 0.05, 3000.0), result("ei", 0.08, 6000.0)];
    assert_eq!(performance_pick(&all[1], &all), None);
}

#[test]
fn exactly_five_percent_is_not_under() {
    let all = [result("zv", 0.01, 3000.0), result("ei", 0.05, 6000.0)];
    assert_eq!(performance_pick(&all[0], &all), None);
}

parity_test! {
    fn matches_shaketune_on_every_golden_case() {
        for (name, want) in &shaketune().cases {
            let case = case(name);
            let data: Vec<_> = case.inputs.iter().map(|i| normalized(stem(i))).collect();
            let rec = find_best_shaper(&data, &fit_params(&case.params)).unwrap();
            let got = rec.for_performance.as_ref().map(|r| r.name.as_str());
            let expected = want.performance.shown.then(|| want.performance.name.as_deref().unwrap());
            assert_eq!(got, expected, "{name}");
        }
    }
}
