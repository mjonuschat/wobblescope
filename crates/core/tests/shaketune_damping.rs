#[macro_use]
mod common;

use common::{case, close, shaketune};
use wobblescope_core::resonance::estimate_damping;

parity_test! {
    fn matches_shaketune_on_every_golden_dataset() {
        let oracle = shaketune();
        assert_eq!(oracle.cases.len(), 12);
        for (name, want) in &oracle.cases {
            let case = case(name);
            assert_eq!(case.datasets.len(), want.datasets.len(), "{name}");
            for (d, w) in case.datasets.iter().zip(&want.datasets) {
                let got = estimate_damping(&d.normalized.freq_bins, &d.normalized.psd_sum).unwrap();
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
    // Rises to its maximum in the last bin: nothing falls to half power after it.
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
