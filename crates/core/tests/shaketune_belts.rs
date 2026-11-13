#[macro_use]
mod common;

use common::{assert_close, close, shaketune};
use wobblescope_core::belts::{
    GRID_POINTS, MAX_PEAKS, PAIR_CAP_HZ, PEAK_THRESHOLD, PEAK_VICINITY, PEAK_WINDOW,
    UNPAIRED_PENALTY, compare_psds,
};

parity_test! {
    fn matches_shaketune_on_every_belt_case() {
        for case in &shaketune().belts {
            let got = compare_psds(
                &case.freqs_a,
                &case.psd_a,
                &case.freqs_b,
                &case.psd_b,
                case.max_freq,
            );
            let want = &case.expected;
            let name = &case.name;
            assert_close(&format!("{name} freqs"), &got.freqs, &want.freqs);
            assert_close(&format!("{name} psd a"), &got.a.psd, &want.psd_a);
            assert_close(&format!("{name} psd b"), &got.b.psd, &want.psd_b);
            assert_eq!(got.a.peaks, want.peaks_a, "{name}: peaks a");
            assert_eq!(got.b.peaks, want.peaks_b, "{name}: peaks b");
            assert_eq!(got.a.unpaired, want.unpaired_a, "{name}: unpaired a");
            assert_eq!(got.b.unpaired, want.unpaired_b, "{name}: unpaired b");
            let pairs: Vec<(usize, usize)> =
                got.pairs.iter().map(|p| (p.a.index, p.b.index)).collect();
            assert_eq!(pairs, want.pairs, "{name}: pairs");
            assert_eq!(got.offsets.len(), got.pairs.len(), "{name}: one offset per pair");
            let highest = got.a.psd.iter().chain(&got.b.psd).fold(f64::MIN, |m, &v| m.max(v));
            for (pair, offset) in got.pairs.iter().zip(&got.offsets) {
                close(
                    &format!("{name} freq delta"),
                    offset.freq_delta,
                    (pair.b.freq - pair.a.freq).abs(),
                );
                let rise = (got.a.psd[pair.b.index] - got.a.psd[pair.a.index]).abs();
                close(
                    &format!("{name} amplitude delta"),
                    offset.amplitude_delta_percent,
                    rise / highest * 100.0,
                );
            }
            match want.similarity {
                Some(s) => close(&format!("{name} similarity"), got.similarity, s),
                None => assert!(got.similarity.is_nan(), "{name}: similarity is NaN"),
            }
            assert_eq!(got.health, want.health, "{name}: health");
            assert_eq!(got.max_freq, case.max_freq);
        }
    }
}

#[test]
fn the_constants_are_shaketunes() {
    assert_eq!(GRID_POINTS, 500);
    assert_eq!(PEAK_THRESHOLD, 0.1);
    assert_eq!(PEAK_WINDOW, 20);
    assert_eq!(PEAK_VICINITY, 15);
    assert_eq!(PAIR_CAP_HZ, 10.0);
    assert_eq!(MAX_PEAKS, 2);
    assert_eq!(UNPAIRED_PENALTY, 30.0);
}
