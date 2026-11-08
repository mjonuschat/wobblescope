#[macro_use]
mod common;

use common::load_csv;
use common::normalized;
use wobblescope_core::eligibility::{Coverage, DatasetInput, Decision, Health};
use wobblescope_core::fit::{FitParams, find_best_shaper, fit_shaper};
use wobblescope_core::multichip::{analyze, summed_psd};
use wobblescope_core::numeric::{arange, interp, median_rate};
use wobblescope_core::score::{estimate_shaper, find_max_accel, shaper_smoothing};
use wobblescope_core::shaper::{Impulses, builtin};
use wobblescope_core::{CalibrationData, CoreError, RawSamples};

fn input(chip: &str, health: Health, samples: RawSamples) -> DatasetInput {
    DatasetInput {
        chip: chip.to_owned(),
        point: 0,
        samples,
        health,
        coverage: Coverage::Complete,
        include_override: false,
        window: None,
        exclusions: Vec::new(),
        load_error: None,
    }
}

/// `n` samples with finite, strictly increasing timestamps from `t0` to `t1`, built by weighted
/// interpolation so `t1 - t0` is never formed (it may overflow even when both ends are finite).
fn samples_over(t0: f64, t1: f64, n: usize) -> RawSamples {
    let t: Vec<f64> = (0..n)
        .map(|i| {
            let u = i as f64 / (n - 1) as f64;
            t0 * (1.0 - u) + t1 * u
        })
        .collect();
    assert!(
        t.iter().all(|v| v.is_finite()),
        "fixture timestamps must be finite"
    );
    assert!(
        t.windows(2).all(|w| w[1] > w[0]),
        "fixture timestamps must increase"
    );
    RawSamples {
        t,
        x: vec![1.0; n],
        y: vec![2.0; n],
        z: vec![3.0; n],
    }
}

parity_test! {
    fn unprocessable_ineligible_capture_does_not_block_a_healthy_chip() {
        let a = analyze(
            &[
                input("good", Health::Ok, load_csv("chip_b")),
                input("dead", Health::Degraded, RawSamples::default()),
            ],
            &FitParams::default(),
        )
        .unwrap();
        assert!(a.worst_case.is_some());
        assert_eq!(a.failures.len(), 1);
        assert_eq!(a.failures[0].chip, "dead");
        assert_eq!(a.per_chip.len(), 2);
        assert!(a.per_chip[1].datasets.is_empty());
        assert!(a.per_chip[1].recommendation.is_none());
    }
}

parity_test! {
    fn an_unprocessable_eligible_capture_is_dropped_and_the_rest_analysed() {
        let good = load_csv("chip_b");
        let a = analyze(
            &[
                input("good", Health::Ok, good.clone()),
                input("dead", Health::Ok, RawSamples::default()),
            ],
            &FitParams::default(),
        )
        .unwrap();
        let alone = analyze(&[input("good", Health::Ok, good)], &FitParams::default()).unwrap();
        assert_eq!(a.worst_case, alone.worst_case);
        let dead = &a.eligibility.decisions[1];
        assert_eq!(dead.decision, Decision::Included);
        assert!(dead.dropped.as_deref().is_some_and(|m| m.contains("too few samples")), "{dead:?}");
        assert!(a.failures.is_empty());
        assert!(a.per_chip[1].recommendation.is_none());
    }
}

parity_test! {
    fn backward_timestamps_drop_the_recording_not_the_axis() {
        let mut bad = load_csv("chip_a");
        bad.t.swap(1000, 1001);
        let a = analyze(
            &[input("a", Health::Ok, bad), input("b", Health::Ok, load_csv("chip_b"))],
            &FitParams::default(),
        )
        .unwrap();
        let alone = analyze(&[input("b", Health::Ok, load_csv("chip_b"))], &FitParams::default())
            .unwrap();
        assert_eq!(a.worst_case, alone.worst_case);
        assert_eq!(a.summed, alone.summed);
        assert!(a.eligibility.decisions[0].dropped.is_some());
        assert!(a.eligibility.decisions[1].dropped.is_none());
        assert!(a.per_chip[1].recommendation.is_some());
    }
}

parity_test! {
    fn an_overridden_recording_that_cannot_be_analysed_is_dropped() {
        let mut bad = load_csv("chip_a");
        bad.t[2000] = bad.t[1999];
        let mut a_in = input("a", Health::Degraded, bad);
        a_in.include_override = true;
        let a = analyze(
            &[a_in, input("b", Health::Ok, load_csv("chip_b"))],
            &FitParams::default(),
        )
        .unwrap();
        let alone = analyze(&[input("b", Health::Ok, load_csv("chip_b"))], &FitParams::default())
            .unwrap();
        assert_eq!(a.eligibility.decisions[0].decision, Decision::Overridden);
        assert!(a.eligibility.decisions[0].dropped.is_some());
        assert_eq!(a.per_chip[1].recommendation, alone.per_chip[0].recommendation);
    }
}

parity_test! {
    fn a_flat_eligible_recording_is_dropped_by_the_fit_check() {
        let mut flat = load_csv("chip_a");
        for column in [&mut flat.x, &mut flat.y, &mut flat.z] {
            column.iter_mut().for_each(|v| *v = 0.0);
        }
        let a = analyze(
            &[input("flat", Health::Ok, flat), input("b", Health::Ok, load_csv("chip_b"))],
            &FitParams::default(),
        )
        .unwrap();
        assert!(a.eligibility.decisions[0].dropped.is_some());
        assert!(!a.per_chip[0].datasets.is_empty(), "the flat PSD is still drawn");
        assert!(a.per_chip[1].recommendation.is_some());
        assert!(a.worst_case.is_some());
    }
}

parity_test! {
    fn the_only_eligible_recording_dropped_leaves_no_recommendation() {
        let a = analyze(
            &[
                input("dead", Health::Ok, RawSamples::default()),
                input("shown", Health::Degraded, load_csv("chip_b")),
            ],
            &FitParams::default(),
        )
        .unwrap();
        assert!(a.worst_case.is_none());
        assert!(a.summed.is_none());
        assert!(a.eligibility.decisions[0].dropped.is_some());
        assert!(!a.per_chip[1].datasets.is_empty(), "the ineligible PSD is still drawn");
    }
}

parity_test! {
    fn a_load_failure_is_dropped_when_eligible_and_listed_when_not() {
        let mut eligible = input("e", Health::Ok, RawSamples::default());
        eligible.load_error = Some("checksum mismatch".to_owned());
        let mut ineligible = input("i", Health::Degraded, RawSamples::default());
        ineligible.load_error = Some("truncated".to_owned());
        let a = analyze(
            &[eligible, ineligible, input("b", Health::Ok, load_csv("chip_b"))],
            &FitParams::default(),
        )
        .unwrap();
        assert_eq!(a.eligibility.decisions[0].dropped.as_deref(), Some("checksum mismatch"));
        assert_eq!(a.failures.len(), 1);
        assert_eq!(a.failures[0].chip, "i");
        assert_eq!(a.failures[0].error, "truncated");
        assert!(a.worst_case.is_some());
    }
}

parity_test! {
    fn invalid_fit_parameters_fail_the_analysis_even_with_nothing_to_fit() {
        let bad = FitParams { scv: -1.0, ..FitParams::default() };
        for inputs in [
            vec![input("x", Health::Degraded, load_csv("chip_b"))],
            vec![input("dead", Health::Ok, RawSamples::default())],
        ] {
            let r = analyze(&inputs, &bad);
            assert!(matches!(r, Err(CoreError::InvalidParameter(_))), "{r:?}");
        }
    }
}

parity_test! {
    fn everything_unprocessable_and_ineligible_gives_no_recommendation() {
        let a = analyze(
            &[input("dead", Health::Degraded, RawSamples::default())],
            &FitParams::default(),
        )
        .unwrap();
        assert!(a.worst_case.is_none());
        assert_eq!(a.failures.len(), 1);
    }
}

parity_test! {
    fn invalid_fit_parameters_are_rejected() {
        let data = [normalized("chip_b")];
        let bad: Vec<FitParams> = vec![
            FitParams { scv: f64::NAN, ..FitParams::default() },
            FitParams { scv: -1.0, ..FitParams::default() },
            FitParams { damping_ratio: Some(1.0), ..FitParams::default() },
            FitParams { damping_ratio: Some(f64::NAN), ..FitParams::default() },
            FitParams { test_damping_ratios: Some(vec![0.1, 1.5]), ..FitParams::default() },
            FitParams { test_damping_ratios: Some(vec![f64::INFINITY]), ..FitParams::default() },
            FitParams { max_freq: Some(f64::NAN), ..FitParams::default() },
            FitParams { max_freq: Some(1e12), ..FitParams::default() },
            FitParams { max_smoothing: Some(f64::NAN), ..FitParams::default() },
            FitParams { max_vibrations: Some(-0.1), ..FitParams::default() },
        ];
        for p in bad {
            assert!(
                matches!(find_best_shaper(&data, &p), Err(CoreError::InvalidParameter(_))),
                "{p:?}"
            );
        }
    }
}

parity_test! {
    fn extreme_timestamp_spans_do_not_panic_in_psd() {
        for (t0, t1) in [(0.0, 1e-300), (-1e308, 1e308), (0.0, 1e300), (0.0, 1e-15)] {
            let r = CalibrationData::from_samples("x", &samples_over(t0, t1, 5000));
            assert!(r.is_err(), "{t0} {t1}");
        }
    }
}

parity_test! {
    fn numeric_helpers_survive_degenerate_input() {
        assert!(interp(&[1.0], &[], &[]).iter().all(|v| v.is_nan()));
        assert!(interp(&[1.0], &[0.0, 1.0], &[0.0]).iter().all(|v| v.is_nan()));
        assert!(arange(0.0, 1e300, 1e-3).is_empty());
        assert!(arange(0.0, f64::INFINITY, 1.0).is_empty());
    }
}

parity_test! {
    fn median_rate_needs_one_positive_step_and_ignores_out_of_order_ones() {
        assert_eq!(median_rate(&[]), None);
        assert_eq!(median_rate(&[1.0]), None);
        assert_eq!(median_rate(&[1.0, 1.0, 1.0]), None);
        assert_eq!(median_rate(&[0.0, 0.5]), Some(2.0));
        let t = [0.0, 0.01, 0.02, 0.02, 0.03, 0.025, 0.04, 0.05];
        assert!((median_rate(&t).unwrap() - 100.0).abs() < 1e-9);
    }
}

parity_test! {
    fn scoring_survives_degenerate_impulses() {
        let empty = Impulses { a: vec![], t: vec![] };
        assert_eq!(estimate_shaper(&empty, 0.1, &[10.0]), vec![1.0]);
        assert_eq!(shaper_smoothing(&empty, 5000.0, 5.0), 0.0);
    }
}

parity_test! {
    fn malformed_dataset_is_rejected_not_indexed() {
        let mut d = normalized("chip_b");
        d.psd_sum.truncate(10);
        let shaper = builtin("zv").unwrap();
        assert!(matches!(
            fit_shaper(shaper, &[d], &FitParams::default()),
            Err(CoreError::InvalidParameter(_))
        ));
        let mut e = normalized("chip_b");
        e.freq_bins.clear();
        e.psd_sum.clear();
        assert!(fit_shaper(shaper, &[e], &FitParams::default()).is_err());
    }
}

parity_test! {
    fn find_max_accel_terminates_when_smoothing_is_always_small() {
        let empty = Impulses { a: vec![], t: vec![] };
        assert!(find_max_accel(&empty, 5.0) >= 1e14);
        let single = Impulses { a: vec![1.0], t: vec![0.0] };
        assert!(find_max_accel(&single, 5.0) >= 1e14);
        for dt in [1e-4, 1e-5, 3.3e-6, 1e-12] {
            let imp = Impulses { a: vec![0.5, 0.5], t: vec![0.0, dt] };
            assert!(find_max_accel(&imp, 5.0).is_finite(), "{dt}");
        }
    }
}

parity_test! {
    fn summed_psd_does_not_extrapolate_beyond_a_chips_nyquist() {
        let low = normalized("chip_b");
        let last = *low.freq_bins.last().unwrap();
        assert!(low.psd_sum[low.psd_sum.len() - 1] > 0.0);
        let s = summed_psd(std::slice::from_ref(&low), last + 100.0).unwrap();
        for (f, v) in s.freq_bins.iter().zip(&s.psd) {
            if *f > last {
                assert_eq!(*v, 0.0, "{f}");
            }
        }
    }
}

parity_test! {
    fn fit_rejects_a_request_whose_retained_results_exceed_the_memory_budget() {
        // Each candidate keeps two arrays on the 0.2 Hz grid; 6 kHz is just over 256 MiB for ZV.
        let p = FitParams { max_freq: Some(6000.0), ..FitParams::default() };
        let r = find_best_shaper(&[normalized("chip_b")], &p);
        assert!(matches!(r, Err(CoreError::InvalidParameter(_))), "{r:?}");
        let ok = FitParams { max_freq: Some(1000.0), ..FitParams::default() };
        assert!(find_best_shaper(&[normalized("chip_b")], &ok).is_ok());
    }
}

parity_test! {
    fn remaining_vibrations_rejects_an_invalid_damping_ratio() {
        use wobblescope_core::score::remaining_vibrations;
        let imp = builtin("zv").unwrap().impulses(40.0, 0.1).unwrap();
        for dr in [1.5, 1.0, -0.1, f64::NAN] {
            let r = remaining_vibrations(&imp, dr, &[10.0], &[1.0]);
            assert!(matches!(r, Err(CoreError::InvalidParameter(_))), "{dr}: {r:?}");
        }
    }
}

parity_test! {
    fn fit_is_invariant_to_psd_scale_even_when_sums_would_overflow() {
        let base = normalized("chip_b");
        let max = base.psd_sum.iter().cloned().fold(0.0, f64::max);
        let mut big = base.clone();
        let k = 1e308 / max;
        for v in big.psd_sum.iter_mut() {
            *v *= k;
        }
        let want = find_best_shaper(&[base], &FitParams::default()).unwrap();
        let got = find_best_shaper(&[big], &FitParams::default()).unwrap();
        assert_eq!(got.best.name, want.best.name);
        assert_eq!(got.best.freq, want.best.freq);
        assert!(got.best.vibrs.is_finite() && got.best.vibrs > 0.0);
        assert!((got.best.vibrs - want.best.vibrs).abs() <= 1e-9 * want.best.vibrs);
    }
}

parity_test! {
    fn interp_rejects_malformed_coordinates_without_panicking() {
        let cases: [(&[f64], &[f64]); 5] = [
            (&[f64::NAN], &[1.0]),
            (&[0.0, f64::NAN], &[1.0, 2.0]),
            (&[1.0, 0.0], &[1.0, 2.0]),
            (&[0.0, 0.0], &[1.0, 2.0]),
            (&[0.0, f64::INFINITY], &[1.0, 2.0]),
        ];
        for (xp, fp) in cases {
            for x in [0.0, 0.5, 1.0, 5.0] {
                let out = interp(&[x], xp, fp);
                assert_eq!(out.len(), 1);
                assert!(out[0].is_nan(), "xp={xp:?} x={x}: {out:?}");
            }
        }
    }
}

parity_test! {
    fn find_max_accel_terminates_for_non_monotonic_smoothing() {
        // Negative amplitudes make smoothing non-monotonic in acceleration: it is under the
        // target at 1e-9 but over it at 1.0 and at every halving of 1.0 above 1e-9.
        let imp = Impulses { a: vec![1.0, -2.0, 4.0, -1.0], t: vec![0.0, 30000.0, 60000.0, 90000.0] };
        let v = find_max_accel(&imp, 0.000047);
        assert!(v.is_finite() && v >= 0.0, "{v}");
    }
}

parity_test! {
    fn remaining_vibrations_rejects_non_finite_or_negative_psd_entries() {
        use wobblescope_core::score::remaining_vibrations;
        let imp = builtin("zv").unwrap().impulses(40.0, 0.1).unwrap();
        let bins = [10.0, 20.0];
        for psd in [[1.0, f64::NAN], [1.0, f64::NEG_INFINITY], [1.0, f64::INFINITY], [1.0, -1.0]] {
            let r = remaining_vibrations(&imp, 0.1, &bins, &psd);
            assert!(matches!(r, Err(CoreError::InvalidParameter(_))), "{psd:?}: {r:?}");
        }
    }
}

parity_test! {
    fn remaining_vibrations_rejects_impulses_with_a_non_finite_response() {
        use wobblescope_core::score::remaining_vibrations;
        for (a, t) in [
            (vec![1.0, f64::NAN], vec![0.0, 0.01]),
            (vec![1.0, -1.0], vec![0.0, 0.01]),
            (vec![1.0, 1.0], vec![0.0, f64::INFINITY]),
        ] {
            let imp = Impulses { a, t };
            let r = remaining_vibrations(&imp, 0.1, &[10.0, 20.0], &[1.0, 1.0]);
            assert!(matches!(r, Err(CoreError::InvalidParameter(_))), "{imp:?}: {r:?}");
        }
    }
}

parity_test! {
    fn memory_budget_counts_the_selected_results_clone() {
        // ZV: 645 candidates on a 26000-bin grid is 268.3 MB (under 256 MiB = 268.4 MB), but the
        // returned vector holds a 646th, cloned result, which takes it to 268.7 MB.
        let p = FitParams { max_freq: Some(5200.0), ..FitParams::default() };
        let r = find_best_shaper(&[normalized("chip_b")], &p);
        assert!(matches!(r, Err(CoreError::InvalidParameter(_))), "{:?}", r.map(|_| ()));
    }
}
