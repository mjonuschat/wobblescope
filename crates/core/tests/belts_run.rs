use wobblescope_core::RawSamples;
use wobblescope_core::belts::compare_psds;
use wobblescope_core::belts_run::analyze_belts;
use wobblescope_core::eligibility::{Coverage, DatasetInput, Decision, Health};
use wobblescope_core::psd::CalibrationData;

const RATE: f64 = 3200.0;

/// Two resonances at `f1` and `f2` Hz plus a deterministic ripple, `seconds` long.
fn tones(f1: f64, f2: f64, seconds: f64, start: f64) -> RawSamples {
    let n = (seconds * RATE) as usize;
    let mut s = RawSamples::default();
    for k in 0..n {
        let t = start + k as f64 / RATE;
        let tau = std::f64::consts::TAU;
        let ripple = (k as f64 * 12.9898).sin() * 0.05;
        s.t.push(t);
        s.x.push((tau * f1 * t).sin() + 0.5 * (tau * f2 * t).sin() + ripple);
        s.y.push(0.3 * (tau * f1 * t).sin() + ripple);
        s.z.push(ripple);
    }
    s
}

fn input(samples: RawSamples) -> DatasetInput {
    DatasetInput {
        chip: "adxl".to_owned(),
        point: 0,
        samples,
        health: Health::Ok,
        coverage: Coverage::Complete,
        include_override: false,
        window: None,
        exclusions: Vec::new(),
        load_error: None,
    }
}

fn degraded(mut i: DatasetInput) -> DatasetInput {
    i.health = Health::Degraded;
    i
}

fn psd_of(i: &DatasetInput) -> CalibrationData {
    CalibrationData::from_samples("x", &i.samples).unwrap()
}

#[test]
fn both_sides_usable_gives_a_result() {
    let a = input(tones(50.0, 90.0, 5.0, 0.0));
    let b = input(tones(52.0, 88.0, 5.0, 0.0));
    let got = analyze_belts("adxl", 0, Some(&a), Some(&b), 200.0);
    let (pa, pb) = (psd_of(&a), psd_of(&b));
    let want = compare_psds(
        &pa.freq_bins,
        &pa.psd_sum,
        &pb.freq_bins,
        &pb.psd_sum,
        200.0,
    );
    assert_eq!(got.result.as_ref(), Some(&want));
    assert!(got.belt_a.present && got.belt_b.present);
    assert_eq!(got.chip, "adxl");
}

#[test]
fn a_missing_side_gives_no_result_and_present_false() {
    let a = input(tones(50.0, 90.0, 5.0, 0.0));
    let got = analyze_belts("adxl", 0, Some(&a), None, 200.0);
    assert!(got.result.is_none());
    assert!(got.belt_a.present);
    assert!(!got.belt_b.present);
    assert!(got.belt_b.decision.is_none() && got.belt_b.spectrum.is_none());
}

#[test]
fn a_usable_side_keeps_its_spectrum_when_the_other_is_missing() {
    let a = input(tones(50.0, 90.0, 5.0, 0.0));
    let got = analyze_belts("adxl", 0, Some(&a), None, 200.0);
    let spectrum = got.belt_a.spectrum.expect("belt A spectrum");
    let pa = psd_of(&a);
    assert!(spectrum.freqs.iter().all(|f| *f <= 200.0));
    assert!(spectrum.freqs.len() < pa.freq_bins.len());
    assert_eq!(spectrum.psd, pa.psd_sum[..spectrum.freqs.len()]);
}

#[test]
fn a_degraded_side_is_excluded_with_its_reason() {
    let a = input(tones(50.0, 90.0, 5.0, 0.0));
    let b = degraded(input(tones(52.0, 88.0, 5.0, 0.0)));
    let got = analyze_belts("adxl", 0, Some(&a), Some(&b), 200.0);
    let d = got.belt_b.decision.unwrap();
    assert_eq!(d.decision, Decision::Excluded);
    assert_eq!(d.reasons, ["degraded capture health"]);
    assert!(got.belt_b.spectrum.is_none());
    assert!(got.belt_a.spectrum.is_some());
    assert!(got.result.is_none());
}

#[test]
fn include_override_brings_a_degraded_side_back() {
    let a = input(tones(50.0, 90.0, 5.0, 0.0));
    let mut b = degraded(input(tones(52.0, 88.0, 5.0, 0.0)));
    b.include_override = true;
    let got = analyze_belts("adxl", 0, Some(&a), Some(&b), 200.0);
    assert_eq!(got.belt_b.decision.unwrap().decision, Decision::Overridden);
    assert!(got.result.is_some());
}

#[test]
fn a_side_that_cannot_make_a_psd_reports_a_failure() {
    let a = input(tones(50.0, 90.0, 5.0, 0.0));
    let b = input(tones(52.0, 88.0, 0.01, 0.0));
    let got = analyze_belts("adxl", 0, Some(&a), Some(&b), 200.0);
    assert!(got.result.is_none());
    let failure = got.belt_b.failure.clone().expect("failure");
    assert_eq!(got.belt_b.decision.unwrap().dropped, Some(failure));
    assert!(got.belt_b.spectrum.is_none());
    assert!(got.belt_a.spectrum.is_some());
}

#[test]
fn the_segment_window_is_applied() {
    let inside = tones(50.0, 90.0, 5.0, 10.0);
    let mut padded = tones(120.0, 150.0, 3.0, 5.0);
    padded.t.extend(&inside.t);
    padded.x.extend(&inside.x);
    padded.y.extend(&inside.y);
    padded.z.extend(&inside.z);
    let mut a = input(padded);
    a.window = Some(10.0..15.0);
    let b = input(tones(52.0, 88.0, 5.0, 0.0));
    let windowed = analyze_belts("adxl", 0, Some(&a), Some(&b), 200.0);
    let clean = analyze_belts("adxl", 0, Some(&input(inside)), Some(&b), 200.0);
    assert_eq!(windowed.result, clean.result);
    assert!(windowed.result.is_some());
}

#[test]
fn flat_curves_give_a_nan_similarity_not_an_error() {
    let flat = |start: f64| {
        let mut s = RawSamples::default();
        for k in 0..16_000 {
            s.t.push(start + k as f64 / RATE);
            s.x.push(0.0);
            s.y.push(0.0);
            s.z.push(0.0);
        }
        input(s)
    };
    let got = analyze_belts("adxl", 0, Some(&flat(0.0)), Some(&flat(0.0)), 200.0);
    assert!(got.result.unwrap().similarity.is_nan());
}
