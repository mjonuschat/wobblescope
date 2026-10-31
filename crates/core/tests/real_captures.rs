#[macro_use]
mod common;

use common::{WINDOW_CASES, load_real, window_markers};
use wobblescope_core::CalibrationData;

parity_test! {
    fn real_captures_decompress_and_yield_a_psd() {
        for (name, min_rate, max_rate) in [
            ("ws_x_adxl345", 3000.0, 3400.0),
            ("ws_y_adxl345", 3000.0, 3400.0),
            ("ws_x_ebb36", 300.0, 500.0),
            ("ws_y_ebb36", 300.0, 500.0),
        ] {
            let s = load_real(name);
            let rate = s.len() as f64 / (s.t[s.len() - 1] - s.t[0]);
            assert!((min_rate..max_rate).contains(&rate), "{name}: {rate} Hz");
            let cd = CalibrationData::from_samples(name, &s).unwrap();
            assert!(cd.psd_sum.iter().all(|v| v.is_finite()), "{name}");
        }
    }
}

parity_test! {
    fn whole_window_timestamps_share_a_clock_with_the_print_time_markers() {
        // Klipper's own file is a subset of the whole-window capture, identical sample for
        // sample, so the capture's timestamps are in Klipper's print_time clock.
        for case in WINDOW_CASES {
            let m = window_markers(case);
            for chip in ["adxl345", "ebb36"] {
                let ours = load_real(&format!("win_{case}_{chip}"));
                let klipper = load_real(&format!("win_{case}_klipper_{chip}"));
                // The recording opens at or before the marker: well before it when the toolhead
                // had to move to the point, at it when it was already there.
                assert!(ours.t[0] < m.p1_move_end + 0.05, "{case} {chip}");
                assert!(ours.t[ours.len() - 1] > m.p2_sweep_end, "{case} {chip}");
                let first = ours.t.partition_point(|&t| t < klipper.t[0]);
                assert_eq!(ours.t[first], klipper.t[0], "{case} {chip}");
                assert_eq!(ours.x[first..first + klipper.len()], klipper.x[..], "{case} {chip}");
                // Klipper's window opens when the vibration starts, about a second after the marker.
                let offset = klipper.t[0] - m.p1_move_end;
                assert!((0.9..1.2).contains(&offset), "{case} {chip}: offset {offset}");
            }
        }
    }
}
