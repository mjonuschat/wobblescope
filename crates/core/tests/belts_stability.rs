#[macro_use]
mod common;

use common::belt_fixtures;
use wobblescope_core::belts::compare_psds_in_band;

const MAX_FREQS: [f64; 9] = [
    140.0, 150.0, 160.0, 170.0, 180.0, 190.0, 200.0, 210.0, 250.0,
];

parity_test! {
    fn the_balanced_capture_reads_the_same_peaks_at_every_max_freq() {
        let fixtures = belt_fixtures("belts_balanced");
        let c = &fixtures[0];
        let compare = |max_freq| {
            compare_psds_in_band(&c.freqs_a, &c.psd_a, &c.freqs_b, &c.psd_b, max_freq, c.sweep)
        };
        let first = compare(MAX_FREQS[0]);
        let hz = |r: &wobblescope_core::belts::BeltsResult| -> Vec<(i64, i64)> {
            r.pairs.iter().map(|p| (p.a.freq.round() as i64, p.b.freq.round() as i64)).collect()
        };
        for max_freq in MAX_FREQS {
            let got = compare(max_freq);
            assert_eq!(got.pairs.len(), first.pairs.len(), "pair count at {max_freq}");
            assert_eq!(got.health, first.health, "health at {max_freq}");
            // A peak sits on the nearest grid point, which moves with `max_freq`.
            for (g, w) in hz(&got).into_iter().zip(hz(&first)) {
                assert!((g.0 - w.0).abs() <= 1 && (g.1 - w.1).abs() <= 1, "{g:?} against {w:?} at {max_freq}");
            }
        }
    }
}

parity_test! {
    fn ripples_on_a_plateau_are_not_peaks() {
        let fixtures = belt_fixtures("belts_balanced");
        let c = &fixtures[0];
        let got = compare_psds_in_band(&c.freqs_a, &c.psd_a, &c.freqs_b, &c.psd_b, 200.0, c.sweep);
        for freq in got.a.peaks.iter().chain(&got.b.peaks).map(|&i| got.freqs[i]) {
            assert!(!(95.0..=115.0).contains(&freq), "a ripple at {freq} Hz is a peak");
        }
        assert_eq!(got.pairs.len(), 3);
    }
}
