mod common;

use common::{load_csv, normalized};
use wobblescope_core::fit::{DampingSource, FitParams, find_best_shaper};
use wobblescope_core::multichip::{Analysis, summed_psd};
use wobblescope_core::resonance::estimate_damping;
use wobblescope_core::{CalibrationData, CoreError, RawSamples};

fn half_rate_chip_a() -> CalibrationData {
    let full = load_csv("chip_a");
    let every_second = |v: &[f64]| v.iter().copied().step_by(2).collect::<Vec<_>>();
    let half = RawSamples {
        t: every_second(&full.t),
        x: every_second(&full.x),
        y: every_second(&full.y),
        z: every_second(&full.z),
    };
    let mut d = CalibrationData::from_samples("half", &half).unwrap();
    d.normalize_to_frequencies();
    d
}

#[test]
fn none_estimates_from_the_chip_data() {
    let a = normalized("chip_a");
    let want = estimate_damping(&a.freq_bins, &a.psd_sum).unwrap();
    let rec = find_best_shaper(&[a], &FitParams::default()).unwrap();
    assert_eq!(rec.settings.damping_source, DampingSource::Estimated);
    assert_eq!(Some(rec.settings.damping_ratio), want.zeta);
    assert_eq!(rec.settings.resonance_freq, Some(want.fr));
}

#[test]
fn some_fixes_the_ratio() {
    let params = FitParams {
        damping_ratio: Some(0.05),
        ..FitParams::default()
    };
    let rec = find_best_shaper(&[normalized("chip_a")], &params).unwrap();
    assert_eq!(rec.settings.damping_source, DampingSource::Fixed);
    assert_eq!(rec.settings.damping_ratio, 0.05);
    assert_eq!(rec.settings.resonance_freq, None);
}

#[test]
fn several_datasets_estimate_from_their_sum() {
    let (a, half) = (normalized("chip_a"), half_rate_chip_a());
    assert!(
        half.freq_bins.last() < a.freq_bins.last(),
        "different grids"
    );
    let top = a
        .freq_bins
        .last()
        .unwrap()
        .max(*half.freq_bins.last().unwrap());
    let sum = summed_psd(&[a.clone(), half.clone()], top).unwrap();
    let want = estimate_damping(&sum.freq_bins, &sum.psd).unwrap();
    let rec = find_best_shaper(&[a, half], &FitParams::default()).unwrap();
    assert_eq!(rec.settings.damping_source, DampingSource::Estimated);
    assert_eq!(Some(rec.settings.damping_ratio), want.zeta);
}

#[test]
fn invalid_ratios_are_refused() {
    for bad in [0.0, 1.0, f64::NAN] {
        let params = FitParams {
            damping_ratio: Some(bad),
            ..FitParams::default()
        };
        let got = find_best_shaper(&[normalized("chip_a")], &params);
        assert!(
            matches!(got, Err(CoreError::InvalidParameter(_))),
            "{bad}: {got:?}"
        );
    }
}

#[test]
fn from_legacy_reads_null_and_zero_as_a_tenth() {
    let with = |damping_ratio| FitParams {
        damping_ratio,
        ..FitParams::default()
    };
    assert_eq!(with(None).from_legacy().damping_ratio, Some(0.1));
    assert_eq!(with(Some(0.0)).from_legacy().damping_ratio, Some(0.1));
    assert_eq!(with(Some(0.05)).from_legacy().damping_ratio, Some(0.05));
}

#[test]
fn an_old_analysis_json_still_deserialises() {
    let old = include_str!("data/legacy/analysis_v1.json");
    let analysis: Analysis = serde_json::from_str(old).unwrap();
    let rec = analysis.per_chip[0].recommendation.as_ref().unwrap();
    assert_eq!(rec.settings.damping_source, DampingSource::Fixed);
    assert_eq!(rec.settings.resonance_freq, None);
}
