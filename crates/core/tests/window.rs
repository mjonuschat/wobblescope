#[macro_use]
mod common;

use common::{WINDOW_CASES, assert_close, energy_between, load_csv, load_real, window_markers};
use wobblescope_core::eligibility::{Coverage, DatasetInput, Health};
use wobblescope_core::fit::FitParams;
use wobblescope_core::multichip::{Analysis, analyze};
use wobblescope_core::numeric::interp;
use wobblescope_core::{CoreError, RawSamples};

fn input(samples: RawSamples, window: Option<std::ops::Range<f64>>) -> DatasetInput {
    DatasetInput {
        chip: "c".to_owned(),
        point: 0,
        samples,
        health: Health::Ok,
        coverage: Coverage::Complete,
        include_override: false,
        window,
    }
}

parity_test! {
    fn a_time_window_selects_samples_by_timestamp() {
        let s = RawSamples {
            t: (0..10).map(|i| i as f64).collect(),
            x: vec![0.0; 10],
            y: vec![0.0; 10],
            z: vec![0.0; 10],
        };
        assert_eq!(s.window(2.0..5.0), Ok(2..5));
        assert_eq!(s.window(2.5..5.5), Ok(3..6));
        assert_eq!(s.window(-5.0..3.0), Ok(0..3));
        assert_eq!(s.window(8.0..99.0), Ok(8..10));
        assert_eq!(s.window(20.0..30.0), Ok(10..10));
    }
}

parity_test! {
    fn malformed_windows_are_rejected() {
        let s = load_real("win_x_short_ebb36");
        for w in [5.0..5.0, 6.0..5.0, f64::NAN..9.0, 1.0..f64::INFINITY] {
            assert_eq!(s.window(w.clone()), Err(CoreError::InvalidTimestamps), "{w:?}");
        }
        let mut unsorted = s.clone();
        unsorted.t.swap(10, 11);
        let m = window_markers("x_short");
        assert_eq!(unsorted.window(m.p1_move_end..m.p2_sweep_end), Err(CoreError::InvalidTimestamps));
    }
}

parity_test! {
    fn a_window_outside_the_data_is_too_few_samples_not_a_panic() {
        let s = load_csv("chip_b");
        let r = analyze(&[input(s, Some(1.0e6..2.0e6))], &FitParams::default());
        assert!(matches!(r, Err(CoreError::TooFewSamples { have: 0, .. })), "{r:?}");
    }
}

parity_test! {
    fn analyze_uses_the_window_and_reports_the_range() {
        let m = window_markers("x_short");
        let full = load_real("win_x_short_adxl345");
        let range = full.window(m.p1_move_end..m.p2_sweep_end).unwrap();
        let run = |s: RawSamples, w| analyze(&[input(s, w)], &FitParams::default()).unwrap();
        let windowed = run(full.clone(), Some(m.p1_move_end..m.p2_sweep_end));
        let pre_sliced = run(full.slice(range.clone()), None);
        let whole = run(full, None);
        assert_eq!(windowed.per_chip[0].windows, vec![(0, range)]);
        assert!(whole.per_chip[0].windows.is_empty());
        let psd = |a: &Analysis| a.per_chip[0].datasets[0].1.psd_sum.clone();
        assert_close("windowed psd", &psd(&windowed), &psd(&pre_sliced));
        assert_ne!(psd(&windowed), psd(&whole));
    }
}

parity_test! {
    fn the_marker_removes_the_positioning_burst_when_there_is_one() {
        // The `_short` captures start away from the point, so Klipper's own move to it is a real
        // burst, 60-400x the quiet pause after it; the marker must leave none of it in.
        for case in ["x_short", "y_short"] {
            let m = window_markers(case);
            for chip in ["adxl345", "ebb36"] {
                let s = load_real(&format!("win_{case}_{chip}"));
                let burst = energy_between(&s, s.t[0] + 0.4..m.p1_move_end - 0.15);
                let kept = s.slice(s.window(m.p1_move_end..m.p2_sweep_end).unwrap());
                let first = energy_between(&kept, m.p1_move_end..m.p1_move_end + 0.7);
                assert!(burst > 20.0 * first, "{case} {chip}: burst {burst:.0} vs kept {first:.0}");
            }
        }
    }
}

/// Mean difference between two PSDs on `reference`'s bins, over `band`, as a fraction of the
/// reference peak (empty bins must not dominate a relative error).
fn psd_difference(a: &Analysis, reference: &Analysis, band: std::ops::RangeInclusive<f64>) -> f64 {
    let (a, r) = (
        &a.per_chip[0].datasets[0].1,
        &reference.per_chip[0].datasets[0].1,
    );
    let on_ref = interp(&r.freq_bins, &a.freq_bins, &a.psd_sum);
    let peak = r.psd_sum.iter().cloned().fold(0.0, f64::max);
    let diffs: Vec<f64> = (0..r.freq_bins.len())
        .filter(|&i| band.contains(&r.freq_bins[i]))
        .map(|i| (on_ref[i] - r.psd_sum[i]).abs() / peak)
        .collect();
    diffs.iter().sum::<f64>() / diffs.len() as f64
}

parity_test! {
    fn a_window_from_the_marker_matches_klipper_s_own_file() {
        // Acceptance criteria, fixed before the data was examined: starting the window at the
        // move-end marker (about 1 s before Klipper's own window opens, so it also keeps the
        // pause) must give the same recommendation, or a near tie of it, and a PSD within 5% of
        // the peak, on average over the sweep band.
        let mut flips = 0;
        for case in WINDOW_CASES {
            let m = window_markers(case);
            for chip in ["adxl345", "ebb36"] {
                let label = format!("{case} {chip}");
                let ours = load_real(&format!("win_{case}_{chip}"));
                let klipper = load_real(&format!("win_{case}_klipper_{chip}"));
                let reference = analyze(&[input(klipper, None)], &FitParams::default()).unwrap();
                let got = analyze(
                    &[input(ours, Some(m.p1_move_end..m.p2_sweep_end))],
                    &FitParams::default(),
                )
                .unwrap();
                let (g, r) = (&got.worst_case.as_ref().unwrap().best, &reference.worst_case.as_ref().unwrap().best);
                if g.name == r.name {
                    assert!((g.freq - r.freq).abs() <= 0.4, "{label}: {} vs {}", g.freq, r.freq);
                } else {
                    // A different shaper is acceptable only as a near tie in Klipper's ranking.
                    let tie = reference.worst_case.as_ref().unwrap().all.iter().find(|c| c.name == g.name).unwrap();
                    assert!((tie.score - r.score).abs() / r.score <= 0.05, "{label}: {} vs {}", g.name, r.name);
                    flips += 1;
                }
                let diff = psd_difference(&got, &reference, 15.0..=60.0);
                assert!(diff < 0.05, "{label}: PSD differs by {diff:.3} of peak");
            }
        }
        assert!(flips <= 2, "{flips} near-tie flips");
    }
}
