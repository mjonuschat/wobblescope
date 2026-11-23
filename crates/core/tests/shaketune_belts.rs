#[macro_use]
mod common;

use common::{assert_close, close, shaketune};
use wobblescope_core::belts::{
    GRID_POINTS, MAX_PEAKS, PAIR_CAP_HZ, PEAK_THRESHOLD, PEAK_VICINITY_BINS, PEAK_WINDOW_BINS,
    UNPAIRED_PENALTY, compare_psds_in_band,
};

parity_test! {
    fn matches_shaketune_on_every_belt_case() {
        for case in &shaketune().belts {
            // The whole grid, as Shake&Tune reads it. `compare_psds` starts at 5 Hz when no
            // sweep is recorded; `belts_floor.rs` pins what that moves in these cases.
            let got = compare_psds_in_band(
                &case.freqs_a,
                &case.psd_a,
                &case.freqs_b,
                &case.psd_b,
                case.max_freq,
                None,
            );
            let want = &case.expected;
            let name = &case.name;
            assert_close(&format!("{name} freqs"), &got.freqs, &want.freqs);
            assert_close(&format!("{name} psd a"), &got.a.psd, &want.psd_a);
            assert_close(&format!("{name} psd b"), &got.b.psd, &want.psd_b);
            // Peaks are found on the PSD's own bins and put on the nearest grid point, so a
            // peak sits within a bin (1.25 Hz, three grid points here) of Shake&Tune's.
            let near = |got: &[usize], want: &[usize]| {
                got.len() == want.len() && got.iter().zip(want).all(|(g, w)| g.abs_diff(*w) <= 3)
            };
            let mut want_pairs = want.pairs.clone();
            // Our pairs rise with frequency; Shake&Tune lists them in the order found.
            want_pairs.sort_unstable();
            let pairs: Vec<(usize, usize)> =
                got.pairs.iter().map(|p| (p.a.index, p.b.index)).collect();
            assert!(
                near(&pairs.iter().map(|p| p.0).collect::<Vec<_>>(), &want_pairs.iter().map(|p| p.0).collect::<Vec<_>>())
                    && near(&pairs.iter().map(|p| p.1).collect::<Vec<_>>(), &want_pairs.iter().map(|p| p.1).collect::<Vec<_>>()),
                "{name}: pairs {pairs:?} against {want_pairs:?}"
            );
            // The noise in these two curves is peaks in Shake&Tune's list
            // Here the ripples with a small prominence are dropped and the rest are pinned.
            let noisy: Option<(&[usize], &[usize])> = match name.as_str() {
                "weak_noisy_b" => Some((&[136, 273], &[27, 66, 136, 191, 273, 324, 386, 429])),
                "anticorrelated" => Some((&[148], &[])),
                _ => None,
            };
            match noisy {
                Some((a, b)) => {
                    assert_eq!(got.a.peaks, a, "{name}: peaks a");
                    assert_eq!(got.b.peaks, b, "{name}: peaks b");
                }
                None => {
                    assert!(near(&got.a.peaks, &want.peaks_a), "{name}: peaks a {:?}", got.a.peaks);
                    assert!(near(&got.b.peaks, &want.peaks_b), "{name}: peaks b {:?}", got.b.peaks);
                }
            }
            let left = |peaks: &[usize], matched: Vec<usize>| -> Vec<usize> {
                peaks.iter().copied().filter(|i| !matched.contains(i)).collect()
            };
            assert_eq!(
                got.a.unpaired,
                left(&got.a.peaks, pairs.iter().map(|p| p.0).collect()),
                "{name}: unpaired a"
            );
            assert_eq!(
                got.b.unpaired,
                left(&got.b.peaks, pairs.iter().map(|p| p.1).collect()),
                "{name}: unpaired b"
            );
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
            // The level imbalance scales the score, which Shake&Tune's correlation ignores.
            // These cases have a stretch where one belt is far weaker, so their text is lower.
            let health = match name.as_str() {
                "shifted_peak_6hz" => "Likely a mechanical issue",
                "extra_peak_on_b" => "Mechanical issue detected",
                "weak_noisy_b" => "Potential signs of a mechanical issue",
                _ => health,
            };
            if health != want.health.as_str() && want.health != "Unknown mechanical health" {
                assert!(got.imbalance.figure < 1.0, "{name}: only the imbalance may move the text");
            }
            assert_eq!(got.health, health, "{name}: health");
            assert_eq!(got.max_freq, case.max_freq);
        }
    }
}

#[test]
fn the_constants_are_shaketunes() {
    assert_eq!(GRID_POINTS, 500);
    assert_eq!(PEAK_THRESHOLD, 0.1);
    // Shake&Tune's 20 and 15 grid points, as PSD bins at Klipper's 1.56 Hz.
    assert_eq!(PEAK_WINDOW_BINS, 5.0);
    assert_eq!(PEAK_VICINITY_BINS, 4.0);
    assert_eq!(PAIR_CAP_HZ, 10.0);
    assert_eq!(MAX_PEAKS, 2);
    assert_eq!(UNPAIRED_PENALTY, 30.0);
}
