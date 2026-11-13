#[macro_use]
mod common;

use common::{case, close, klipper_params, load_csv, normalized, shaketune, stem};
use wobblescope_core::eligibility::{Coverage, DatasetInput, Health};
use wobblescope_core::multichip::analyze;
use wobblescope_core::peaks::{
    PEAK_DETECTION, PEAK_EFFECT, detect_peaks, detect_peaks_with, psd_peaks,
};

parity_test! {
    fn matches_shaketune_on_every_golden_dataset() {
        let oracle = shaketune();
        for (name, want) in &oracle.cases {
            let case = case(name);
            for (input, w) in case.inputs.iter().zip(&want.datasets) {
                let got = psd_peaks(&normalized(stem(input)), 200.0).unwrap();
                let label = format!("{name} {}", w.name);
                close(&format!("{label} detect"), got.detect_threshold, w.detect_threshold);
                close(&format!("{label} effect"), got.effect_threshold, w.effect_threshold);
                assert_eq!(got.peaks.len(), w.peaks.len(), "{label}: peak count");
                for (g, e) in got.peaks.iter().zip(&w.peaks) {
                    assert_eq!(g.freq, e.freq, "{label} freq");
                    close(&format!("{label} level"), g.level, e.level);
                    assert_eq!(g.above_effect, e.above_effect, "{label} flag at {}", g.freq);
                }
            }
        }
    }
}

parity_test! {
    fn matches_shaketune_on_the_edge_cases() {
        let edges = shaketune().edge_cases;
        assert_eq!(edges.len(), 5);
        for edge in &edges {
            let got = detect_peaks(&edge.psd, edge.detect_threshold);
            assert_eq!(got.len(), edge.peaks.len(), "{}: peak count", edge.name);
            for (i, e) in got.iter().zip(&edge.peaks) {
                assert_eq!(edge.freq_bins[*i], e.freq, "{} freq", edge.name);
                assert_eq!(edge.psd[*i], e.level, "{} level", edge.name);
                assert_eq!(edge.psd[*i] > edge.effect_threshold, e.above_effect, "{}", edge.name);
            }
        }
    }
}

#[test]
fn the_constants_are_shaketunes() {
    assert_eq!((PEAK_DETECTION, PEAK_EFFECT), (0.05, 0.12));
}

#[test]
fn a_flat_psd_has_no_peaks() {
    assert!(detect_peaks(&[1.0; 50], 0.05).is_empty());
}

#[test]
fn fewer_than_five_bins_has_no_peaks() {
    for n in 0..5 {
        assert!(detect_peaks(&vec![1.0; n], 0.0).is_empty(), "{n} bins");
    }
}

#[test]
fn two_peaks_four_bins_apart_merge_into_the_stronger() {
    let mut psd = vec![0.0; 64];
    psd[20] = 1.0;
    psd[24] = 0.9;
    assert_eq!(detect_peaks(&psd, 0.05), vec![20]);
}

#[test]
fn a_peak_in_the_last_two_bins_is_not_reported() {
    let mut psd = vec![0.0; 64];
    psd[62] = 1.0;
    psd[63] = 1.0;
    assert!(detect_peaks(&psd, 0.05).is_empty());
}

#[test]
fn analyze_gives_every_dataset_its_peaks() {
    let input = |chip: &str| DatasetInput {
        chip: chip.to_owned(),
        point: 0,
        samples: load_csv(chip),
        health: Health::Ok,
        coverage: Coverage::Complete,
        include_override: false,
        window: None,
        exclusions: Vec::new(),
        load_error: None,
    };
    let a = analyze(&[input("chip_a"), input("chip_b")], &klipper_params()).unwrap();
    for (chip, name) in a.per_chip.iter().zip(["chip_a", "chip_b"]) {
        assert_eq!(chip.peaks.len(), 1, "{name}");
        let (point, peaks) = &chip.peaks[0];
        assert_eq!(*point, 0);
        assert_eq!(peaks, &psd_peaks(&chip.datasets[0].1, 200.0).unwrap());
    }
}

#[test]
fn window_and_vicinity_are_parameters() {
    let bump = |i: usize, c: f64| (-((i as f64 - c).powi(2)) / 8.0).exp();
    let psd: Vec<f64> = (0..500)
        .map(|i| bump(i, 200.0) + 0.8 * bump(i, 210.0))
        .collect();
    assert_eq!(detect_peaks_with(&psd, 0.1, 5, 3), detect_peaks(&psd, 0.1));
    assert_eq!(detect_peaks(&psd, 0.1).len(), 2);
    assert_eq!(detect_peaks_with(&psd, 0.1, 20, 15).len(), 1);
}
