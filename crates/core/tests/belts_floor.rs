//! The 5 Hz floor that a session without a recorded sweep gets.
#[macro_use]
mod common;

use common::{belt_fixtures, shaketune};
use wobblescope_core::belts::{BeltsResult, SWEEP_FLOOR_HZ, compare_psds, compare_psds_in_band};

/// 500 bins of 0.5 Hz, 0 to 249.5 Hz: with `MAX_FREQ` they are exactly the 500-point grid, so
/// a curve's value at a grid point is the value written here.
const MAX_FREQ: f64 = 249.5;

fn freqs() -> Vec<f64> {
    (0..500).map(|i| i as f64 * 0.5).collect()
}

/// Sum of triangles `(centre, half width, height)`, zero elsewhere.
fn triangles(parts: &[(f64, f64, f64)]) -> Vec<f64> {
    freqs()
        .iter()
        .map(|f| {
            parts
                .iter()
                .map(|(c, w, h)| h * (1.0 - (f - c).abs() / w).max(0.0))
                .sum()
        })
        .collect()
}

fn compare(a: &[f64], b: &[f64]) -> BeltsResult {
    let f = freqs();
    compare_psds(&f, a, &f, b, MAX_FREQ)
}

fn peak_freqs(got: &BeltsResult) -> Vec<f64> {
    got.a
        .peaks
        .iter()
        .chain(&got.b.peaks)
        .map(|&i| got.freqs[i])
        .collect()
}

parity_test! {
    fn a_tall_two_hz_bump_does_not_hide_an_in_band_peak() {
        // Belt A's bump at 2 Hz is ten times its resonance at 60 Hz, so counted in the
        // threshold it would put the resonance (100) under 10 % of 1000 and hide it.
        let a = triangles(&[(60.0, 8.0, 100.0), (2.0, 1.5, 1000.0)]);
        let b = triangles(&[(60.0, 8.0, 90.0)]);
        let got = compare(&a, &b);
        assert!(peak_freqs(&got).iter().all(|f| *f >= SWEEP_FLOOR_HZ), "{:?}", peak_freqs(&got));
        assert_eq!(got.a.peaks.len(), 1);
        assert_eq!(got.pairs.len(), 1);
        assert_eq!(got.pairs[0].a.freq, 60.0);
        assert_eq!(got.pairs[0].b.freq, 60.0);
    }
}

parity_test! {
    fn without_a_sweep_the_similarity_and_the_offset_normalization_start_at_five_hz() {
        // Belt A: a resonance of 100 at 60 Hz and a bump of 1000 at 2 Hz. Belt B: 90 at 60 Hz
        // and an unpaired resonance of 100 at 150 Hz. The highest in-band value is 100.
        let a = triangles(&[(60.0, 8.0, 100.0), (2.0, 1.5, 1000.0)]);
        let b = triangles(&[(60.0, 8.0, 90.0), (150.0, 3.0, 100.0)]);
        let got = compare(&a, &b);

        // The pair at 60 Hz differs by 90 - 100, a tenth of the in-band maximum 100. Over the
        // whole grid the maximum would be the bump, 1000, and the offset 1 %.
        assert_eq!(got.offsets.len(), 1);
        assert!((got.offsets[0].amplitude_delta_percent - 10.0).abs() < 1e-9, "{:?}", got.offsets);

        // Pearson correlation of the two curves from 5 Hz up, computed outside this code
        // (numpy); over the whole grid it is 15.23.
        assert!((got.similarity - 81.52601096407666).abs() < 1e-6, "{}", got.similarity);

        // Belt B's resonance at 150 Hz is unpaired.
        assert_eq!(got.a.unpaired.len(), 0);
        assert_eq!(got.b.unpaired.len(), 1);
        assert_eq!(got.freqs[got.b.unpaired[0]], 150.0);
    }
}

parity_test! {
    fn a_sweep_recorded_from_below_five_hz_keeps_its_peaks() {
        let a = triangles(&[(60.0, 8.0, 100.0), (2.0, 1.5, 1000.0)]);
        let b = triangles(&[(60.0, 8.0, 90.0), (2.0, 1.5, 900.0)]);
        let f = freqs();
        let got = compare_psds_in_band(&f, &a, &f, &b, MAX_FREQ, Some((1.0, MAX_FREQ)));
        assert!(peak_freqs(&got).contains(&2.0), "{:?}", peak_freqs(&got));
        assert_eq!(got.pairs.len(), 1);
        assert_eq!(got.pairs[0].a.freq, 2.0);
    }
}

parity_test! {
    fn the_unequal_capture_no_longer_pairs_the_two_hz_bump() {
        // The capture records no sweep, so compare_psds gives it the floor.
        for f in belt_fixtures("belts_unequal_power") {
            assert!(f.sweep.is_none(), "{}", f.name);
            for max_freq in [150.0, 200.0] {
                let got = compare_psds(&f.freqs_a, &f.psd_a, &f.freqs_b, &f.psd_b, max_freq);
                let label = format!("{} at {max_freq}", f.name);
                assert!(
                    got.pairs.iter().all(|p| p.a.freq >= SWEEP_FLOOR_HZ && p.b.freq >= SWEEP_FLOOR_HZ),
                    "{label}: {:?}",
                    got.pairs
                );
                assert!(peak_freqs(&got).iter().all(|f| *f >= SWEEP_FLOOR_HZ), "{label}");
            }
        }
    }
}

parity_test! {
    fn the_floor_moves_only_the_similarity_on_the_shaketune_cases() {
        for case in &shaketune().belts {
            let name = &case.name;
            let (fa, pa, fb, pb) = (&case.freqs_a, &case.psd_a, &case.freqs_b, &case.psd_b);
            let floored = compare_psds(fa, pa, fb, pb, case.max_freq);
            let whole = compare_psds_in_band(fa, pa, fb, pb, case.max_freq, None);
            // Belt B of `anticorrelated` has a peak at grid point 8, 3.2 Hz, that the floor drops.
            let mut whole_peaks_b = whole.b.peaks.clone();
            let mut whole_unpaired_b = whole.b.unpaired.clone();
            if name == "anticorrelated" {
                whole_peaks_b.retain(|&i| i != 8);
                whole_unpaired_b.retain(|&i| i != 8);
            }
            assert_eq!(floored.a.peaks, whole.a.peaks, "{name}: peaks a");
            assert_eq!(floored.b.peaks, whole_peaks_b, "{name}: peaks b");
            assert_eq!(floored.b.unpaired, whole_unpaired_b, "{name}: unpaired b");
            assert_eq!(floored.pairs, whole.pairs, "{name}: pairs");
            assert_eq!(floored.offsets, whole.offsets, "{name}: offsets");
            // Dropping 0 to 5 Hz of 500 points shifts the correlation by a fraction of a point.
            assert!((floored.similarity - whole.similarity).abs() < 0.2, "{name}");
        }
    }
}
