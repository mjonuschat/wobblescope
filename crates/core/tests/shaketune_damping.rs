#[macro_use]
mod common;

use common::{case, close, shaketune};
use wobblescope_core::resonance::{estimate_damping, estimate_damping_above};

parity_test! {
    fn matches_shaketune_on_every_golden_dataset() {
        let oracle = shaketune();
        assert_eq!(oracle.cases.len(), 12);
        for (name, want) in &oracle.cases {
            let case = case(name);
            assert_eq!(case.datasets.len(), want.datasets.len(), "{name}");
            for (d, w) in case.datasets.iter().zip(&want.datasets) {
                // `estimate_damping_above` keeps Shake&Tune's half-power level; `estimate_damping` does not.
                let got = estimate_damping_above(&d.normalized.freq_bins, &d.normalized.psd_sum, 0.0)
                    .unwrap();
                let label = format!("{name} {}", w.name);
                assert_eq!(got.fr, w.fr, "{label} fr");
                match (got.zeta, w.zeta) {
                    (Some(g), Some(e)) => close(&format!("{label} zeta"), g, e),
                    (None, None) => {}
                    (g, e) => panic!("{label}: zeta {g:?}, Shake&Tune {e:?}"),
                }
            }
        }
    }
}

#[test]
fn empty_or_mismatched_input_is_none() {
    assert!(estimate_damping(&[], &[]).is_none());
    assert!(estimate_damping(&[1.0, 2.0], &[1.0]).is_none());
}

#[test]
fn a_flat_psd_has_no_zeta() {
    let freqs: Vec<f64> = (0..50).map(f64::from).collect();
    let got = estimate_damping(&freqs, &[2.0; 50]).unwrap();
    assert_eq!(got.fr, 0.0);
    assert_eq!(got.zeta, None);
}

#[test]
fn a_peak_with_no_half_power_point_on_one_side_has_no_zeta() {
    let freqs: Vec<f64> = (0..50).map(f64::from).collect();
    // Rises to its maximum in the last bin, so nothing falls to half power after it.
    let psd: Vec<f64> = (0..50).map(f64::from).collect();
    let got = estimate_damping(&freqs, &psd).unwrap();
    assert_eq!(got.fr, 49.0);
    assert_eq!(got.zeta, None);
}

#[test]
fn a_single_bin_has_no_zeta() {
    let got = estimate_damping(&[5.0], &[1.0]).unwrap();
    assert_eq!(got.fr, 5.0);
    assert_eq!(got.zeta, None);
}

fn resonator_psd(f0: f64, zeta: f64) -> (Vec<f64>, Vec<f64>) {
    let freqs: Vec<f64> = (0..=4000).map(|i| f64::from(i) * 0.05).collect();
    let psd = freqs
        .iter()
        .map(|f| {
            let r = f / f0;
            1.0 / ((1.0 - r * r).powi(2) + (2.0 * zeta * r).powi(2))
        })
        .collect();
    (freqs, psd)
}

#[test]
fn the_shaper_estimate_recovers_the_damping_ratio_of_a_power_spectrum() {
    for zeta in [0.05, 0.1, 0.15] {
        let (freqs, psd) = resonator_psd(50.0, zeta);
        let got = estimate_damping(&freqs, &psd).unwrap().zeta.unwrap();
        assert!((got - zeta).abs() < 0.03 * zeta, "zeta {zeta}: got {got}");
    }
}

#[test]
fn shaketunes_level_underestimates_a_power_spectrum_by_a_quarter_or_more() {
    for zeta in [0.05, 0.1, 0.15] {
        let (freqs, psd) = resonator_psd(50.0, zeta);
        let got = estimate_damping_above(&freqs, &psd, 0.0)
            .unwrap()
            .zeta
            .unwrap();
        assert!(got < 0.8 * zeta, "zeta {zeta}: got {got}");
    }
}

#[test]
fn a_second_peak_close_to_the_first_is_not_measured_as_part_of_it() {
    // Two resonators of damping ratio 0.1, 70 and 90 Hz: the spectrum between them stays above
    // half the peak, so walking on to half would measure one wide peak (a ratio of about 0.23).
    let (freqs, first) = resonator_psd(70.0, 0.1);
    let (_, second) = resonator_psd(90.0, 0.1);
    let psd: Vec<f64> = first.iter().zip(&second).map(|(a, b)| a + b).collect();
    let peak = psd.iter().copied().fold(0.0, f64::max);
    let valley = freqs
        .iter()
        .zip(&psd)
        .filter(|(f, _)| (75.0..85.0).contains(*f))
        .map(|(_, p)| *p)
        .fold(f64::MAX, f64::min);
    assert!(valley > 0.5 * peak, "the dip must stay above half the peak");
    let got = estimate_damping(&freqs, &psd).unwrap().zeta.unwrap();
    assert!((0.05..0.17).contains(&got), "got {got}");
}

#[test]
fn ripple_on_the_skirt_of_a_single_peak_does_not_end_the_measurement() {
    for zeta in [0.05, 0.1, 0.15] {
        let (freqs, clean) = resonator_psd(50.0, zeta);
        let peak = clean.iter().copied().fold(0.0, f64::max);
        let rippled: Vec<f64> = clean
            .iter()
            .enumerate()
            .map(|(i, p)| p + if i % 2 == 0 { 0.015 } else { -0.015 } * peak)
            .collect();
        let want = estimate_damping(&freqs, &clean).unwrap().zeta.unwrap();
        let got = estimate_damping(&freqs, &rippled).unwrap().zeta.unwrap();
        assert!(
            (got - want).abs() < 0.05 * want,
            "zeta {zeta}: {got} against {want}"
        );
    }
}
