//! The live binding reports a tone's peak, waits out a gap and refuses oversized windows.

use wobblescope_wasm::Live;

const RATE: f64 = 800.0;

fn tone(hz: f64, start: usize, n: usize) -> Vec<f32> {
    (start..start + n)
        .flat_map(|i| {
            let v = (std::f64::consts::TAU * hz * i as f64 / RATE).sin() as f32;
            [v, 0.0, 0.0]
        })
        .collect()
}

#[test]
fn a_60_hz_tone_peaks_within_one_bin_of_60_hz() {
    let mut live = Live::new(RATE, 2.0, 200.0).unwrap();
    live.push(0.0, &tone(60.0, 0, 1600)).unwrap();
    let spectrum = live.spectrum().expect("a full window");
    let bin = spectrum.freq_bins[1];
    let peaks = live.peaks(1, 5.0);
    assert_eq!(peaks.len(), 1);
    assert!((peaks[0].freq - 60.0).abs() <= bin, "{}", peaks[0].freq);
}

#[test]
fn a_gap_leaves_no_spectrum() {
    let mut live = Live::new(RATE, 2.0, 200.0).unwrap();
    live.push(0.0, &tone(60.0, 0, 1600)).unwrap();
    assert!(live.spectrum().is_some());
    live.push(10.0, &tone(60.0, 0, 100)).unwrap();
    assert!(live.spectrum().is_none());
    assert!(live.peaks(3, 5.0).is_empty());
}

#[test]
fn a_window_above_the_sample_cap_is_refused() {
    let err = Live::new(RATE, 5_001.0, 200.0).unwrap_err();
    assert!(err.to_string().contains("exceeds"), "{err}");
    assert!(Live::new(RATE, 5_000.0, 200.0).is_ok());
}
