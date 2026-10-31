#[macro_use]
mod common;

use common::load_csv;
use common::normalized;
use wobblescope_core::eligibility::{Coverage, DatasetInput, Health};
use wobblescope_core::fit::{FitParams, find_best_shaper, fit_shaper};
use wobblescope_core::multichip::{analyze, summed_psd};
use wobblescope_core::numeric::{arange, interp};
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
    fn unprocessable_eligible_capture_is_an_error_not_silently_dropped() {
        let r = analyze(
            &[
                input("good", Health::Ok, load_csv("chip_b")),
                input("dead", Health::Ok, RawSamples::default()),
            ],
            &FitParams::default(),
        );
        assert!(matches!(r, Err(CoreError::TooFewSamples { have: 0, .. })));
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
