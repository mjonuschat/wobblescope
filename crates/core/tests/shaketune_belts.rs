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
            // The first peak of `anticorrelated` b is refined from bin 7 to its largest
            // value at bin 8; the rest match. It is also the first unpaired one.
            let (mut peaks_b, mut unpaired_b) = (want.peaks_b.clone(), want.unpaired_b.clone());
            if name == "anticorrelated" {
                assert_eq!((peaks_b[0], unpaired_b[0]), (7, 7));
                (peaks_b[0], unpaired_b[0]) = (8, 8);
            }
            // A bin that several peaks refine to is listed once. Shake&Tune leaves the other
            // copies of a matched bin behind as unpaired, so those drop out of the list too.
            let once = |list: &[usize], matched: &[usize]| {
                let mut out: Vec<usize> = Vec::new();
                for &i in list {
                    if !out.contains(&i) && !matched.contains(&i) {
                        out.push(i);
                    }
                }
                out
            };
            let matched_a: Vec<usize> = want.pairs.iter().map(|p| p.0).collect();
            let matched_b: Vec<usize> = want.pairs.iter().map(|p| p.1).collect();
            // The centred, whole-curve smoothing drops these noise peaks of a curve.
            let dropped_b: &[usize] = if name == "weak_noisy_b" { &[344, 409] } else { &[] };
            let mut matched_b = matched_b;
            matched_b.extend_from_slice(dropped_b);
            let peaks_a = once(&want.peaks_a, &[]);
            let peaks_b = once(&peaks_b, dropped_b);
            let unpaired_a = once(&want.unpaired_a, &matched_a);
            let unpaired_b = once(&unpaired_b, &matched_b);
            assert_eq!(got.a.peaks, peaks_a, "{name}: peaks a");
            assert_eq!(got.b.peaks, peaks_b, "{name}: peaks b");
            assert_eq!(got.a.unpaired, unpaired_a, "{name}: unpaired a");
            assert_eq!(got.b.unpaired, unpaired_b, "{name}: unpaired b");
            let pairs: Vec<(usize, usize)> =
                got.pairs.iter().map(|p| (p.a.index, p.b.index)).collect();
            // Our pairs rise with frequency; Shake&Tune lists them in the order found.
            let mut want_pairs = want.pairs.clone();
            want_pairs.sort_unstable();
            assert_eq!(pairs, want_pairs, "{name}: pairs");
            assert_eq!(got.offsets.len(), got.pairs.len(), "{name}: one offset per pair");
            let highest = got.a.psd.iter().chain(&got.b.psd).fold(f64::MIN, |m, &v| m.max(v));
            for (pair, offset) in got.pairs.iter().zip(&got.offsets) {
                close(
                    &format!("{name} freq delta"),
                    offset.freq_delta,
                    (pair.b.freq - pair.a.freq).abs(),
                );
                // Belt B is read at its own peak; Shake&Tune reads belt A at both.
                let rise = (got.b.psd[pair.b.index] - got.a.psd[pair.a.index]).abs();
                close(
                    &format!("{name} amplitude delta"),
                    offset.amplitude_delta_percent,
                    rise / highest * 100.0,
                );
            }
            match want.similarity {
                Some(s) => close(&format!("{name} similarity"), got.similarity, s),
                // A flat curve scores 0 where Shake&Tune has NaN.
                None => assert_eq!(got.similarity, 0.0, "{name}: similarity"),
            }
            // A score of exactly 0 reads as an issue where Shake&Tune has "Unknown". Both
            // golden cases that read Unknown (`anticorrelated`, `flat_b`) score 0.
            let health = match want.health.as_str() {
                "Unknown mechanical health" => "Mechanical issue detected",
                other => other,
            };
            assert_eq!(got.health, health, "{name}: health");
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
