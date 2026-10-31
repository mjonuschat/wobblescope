#[macro_use]
mod common;

use common::{Psd, assert_close, case, load_csv};
use wobblescope_core::numeric::arange;
use wobblescope_core::{CalibrationData, CoreError, RawSamples};

fn check(label: &str, cd: &CalibrationData, want: &Psd) {
    assert_close(
        &format!("{label} freq_bins"),
        &cd.freq_bins,
        &want.freq_bins,
    );
    assert_close(&format!("{label} psd_x"), &cd.psd_x, &want.psd_x);
    assert_close(&format!("{label} psd_y"), &cd.psd_y, &want.psd_y);
    assert_close(&format!("{label} psd_z"), &cd.psd_z, &want.psd_z);
    assert_close(&format!("{label} psd_sum"), &cd.psd_sum, &want.psd_sum);
}

fn synthetic(n: usize, fs: f64) -> RawSamples {
    RawSamples {
        t: (0..n).map(|i| 100.0 + i as f64 / fs).collect(),
        x: (0..n).map(|i| (i as f64 * 0.37).sin()).collect(),
        y: (0..n).map(|i| (i as f64 * 0.11).cos()).collect(),
        z: vec![1.0; n],
    }
}

parity_test! {
    fn psd_matches_klipper_before_and_after_normalisation() {
        let mut cases = vec![case("chip_a+chip_b")];
        cases.extend(["real_x_both", "real_y_both"].map(case));
        for ds in cases.iter().flat_map(|c| &c.datasets) {
            let mut cd = CalibrationData::from_samples(&ds.name, &load_csv(&ds.name)).unwrap();
            check(&format!("{} raw", ds.name), &cd, &ds.raw);
            cd.normalize_to_frequencies();
            check(&format!("{} normalized", ds.name), &cd, &ds.normalized);
        }
    }
}

parity_test! {
    fn normalisation_is_idempotent() {
        let mut once = CalibrationData::from_samples("a", &load_csv("chip_b")).unwrap();
        once.normalize_to_frequencies();
        let mut twice = once.clone();
        twice.normalize_to_frequencies();
        assert_eq!(once, twice);
    }
}

parity_test! {
    fn rejects_recording_shorter_than_one_window() {
        let err = CalibrationData::from_samples("a", &synthetic(1000, 3200.0)).unwrap_err();
        assert!(matches!(err, CoreError::TooFewSamples { have: 1000, .. }));
    }
}

parity_test! {
    fn rejects_empty_recording() {
        let err = CalibrationData::from_samples("a", &RawSamples::default()).unwrap_err();
        assert!(matches!(err, CoreError::TooFewSamples { have: 0, .. }));
    }
}

parity_test! {
    fn rejects_non_increasing_timestamps() {
        let mut s = synthetic(4000, 3200.0);
        s.t = vec![5.0; 4000];
        assert_eq!(CalibrationData::from_samples("a", &s), Err(CoreError::InvalidTimestamps));
        let one = synthetic(1, 3200.0);
        assert_eq!(CalibrationData::from_samples("a", &one), Err(CoreError::InvalidTimestamps));
    }
}

parity_test! {
    fn rejects_interior_duplicate_and_backwards_timestamps() {
        let mut dup = synthetic(4000, 3200.0);
        dup.t[2000] = dup.t[1999];
        assert_eq!(CalibrationData::from_samples("a", &dup), Err(CoreError::InvalidTimestamps));
        let mut back = synthetic(4000, 3200.0);
        back.t[2000] = back.t[1990];
        assert_eq!(CalibrationData::from_samples("a", &back), Err(CoreError::InvalidTimestamps));
    }
}

parity_test! {
    fn rejects_non_finite_timestamps() {
        let mut s = synthetic(4000, 3200.0);
        s.t[10] = f64::NAN;
        assert_eq!(CalibrationData::from_samples("a", &s), Err(CoreError::InvalidTimestamps));
    }
}

parity_test! {
    fn rejects_mismatched_column_lengths() {
        let mut s = synthetic(4000, 3200.0);
        s.y.pop();
        assert_eq!(CalibrationData::from_samples("a", &s), Err(CoreError::MismatchedColumns));
    }
}

parity_test! {
    fn arange_matches_numpy_tail_and_length() {
        let v = arange(23.0, 150.0, 0.2);
        assert_eq!(v.len(), 635);
        assert_eq!(*v.last().unwrap(), 149.79999999999956);
    }
}

parity_test! {
    fn rejects_non_finite_samples() {
        let mut s = synthetic(4000, 3200.0);
        s.x[10] = f64::NAN;
        assert_eq!(CalibrationData::from_samples("a", &s), Err(CoreError::NonFiniteSample));
    }
}
