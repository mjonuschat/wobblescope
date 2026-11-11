//! Tests for the rolling live spectrum.

use std::f64::consts::PI;

use wobblescope_core::decimate::Decimator;
use wobblescope_core::spectrum::{LiveSpectrum, top_peaks};
use wobblescope_core::{CoreError, RawSamples};

const RATE: f64 = 533.33;

/// Interleaved samples of `tones` (frequency, amplitude) on all three axes.
fn interleaved(tones: &[(f64, f64)], rate: f64, start: usize, n: usize) -> Vec<f32> {
    (start..start + n)
        .flat_map(|i| {
            let t = i as f64 / rate;
            let v = tones
                .iter()
                .map(|(f, a)| a * (2.0 * PI * f * t).sin())
                .sum::<f64>() as f32;
            [v, v, v]
        })
        .collect()
}

fn pushed(tones: &[(f64, f64)], seconds: f64) -> LiveSpectrum {
    let mut live = LiveSpectrum::new(RATE, 1.0, 200.0).unwrap();
    let n = (RATE * seconds) as usize;
    let mut i = 0;
    while i < n {
        let len = 50.min(n - i);
        live.push_f32(i as f64 / RATE, &interleaved(tones, RATE, i, len))
            .unwrap();
        i += len;
    }
    live
}

#[test]
fn a_60_hz_tone_peaks_at_60_hz() {
    let s = pushed(&[(60.0, 1.0)], 2.0).spectrum().unwrap();
    let peaks = top_peaks(&s.freq_bins, &s.psd_sum, 1, 5.0);
    let bin = s.freq_bins[1];
    assert!((peaks[0].freq - 60.0).abs() <= bin, "{:?}", peaks);
}

#[test]
fn the_spectrum_integrates_to_the_variance() {
    let s = pushed(&[(60.0, 1.0)], 2.0).spectrum().unwrap();
    let width = s.freq_bins[1];
    let power: f64 = s.psd_x.iter().sum::<f64>() * width;
    assert!((power - 0.5).abs() < 0.025, "{power}");
}

#[test]
fn no_spectrum_before_a_full_window() {
    let tones = [(60.0, 1.0)];
    let mut live = LiveSpectrum::new(RATE, 1.0, 200.0).unwrap();
    live.push_f32(0.0, &interleaved(&tones, RATE, 0, 267))
        .unwrap();
    assert!(live.spectrum().is_none());
    live.push_f32(267.0 / RATE, &interleaved(&tones, RATE, 267, 267))
        .unwrap();
    assert!(live.spectrum().is_some());
}

#[test]
fn a_gap_resets_the_buffer() {
    let tones = [(60.0, 1.0)];
    let mut live = pushed(&tones, 1.0);
    assert!(live.spectrum().is_some());
    live.push_f32(3.0, &interleaved(&tones, RATE, 100, 100))
        .unwrap();
    assert!(live.spectrum().is_none());
    live.push_f32(3.0 + 100.0 / RATE, &interleaved(&tones, RATE, 200, 500))
        .unwrap();
    assert!(live.spectrum().is_some());
}

#[test]
fn bad_input_changes_nothing() {
    let tones = [(60.0, 1.0)];
    let clean = pushed(&tones, 1.0).spectrum().unwrap();

    let mut live = LiveSpectrum::new(RATE, 1.0, 200.0).unwrap();
    let n = RATE as usize;
    let mut bad = interleaved(&tones, RATE, 0, n);
    bad[4] = f32::NAN;
    assert_eq!(live.push_f32(0.0, &bad), Err(CoreError::NonFiniteSample));
    assert_eq!(
        live.push_f32(0.0, &bad[..4]),
        Err(CoreError::MismatchedColumns)
    );
    assert!(live.spectrum().is_none());
    for i in (0..n).step_by(50) {
        let len = 50.min(n - i);
        live.push_f32(i as f64 / RATE, &interleaved(&tones, RATE, i, len))
            .unwrap();
    }
    assert_eq!(live.spectrum().unwrap(), clean);
}

#[test]
fn peaks_respect_separation_and_order() {
    let s = pushed(&[(40.0, 2.0), (43.0, 1.0)], 2.0).spectrum().unwrap();
    let wide = top_peaks(&s.freq_bins, &s.psd_x, 5, 5.0);
    assert!((wide[0].freq - 40.0).abs() < 2.0);
    for (i, a) in wide.iter().enumerate() {
        assert!(wide[i + 1..].iter().all(|b| (a.freq - b.freq).abs() >= 5.0));
        assert!(i == 0 || (a.freq - 43.0).abs() > 2.0, "{wide:?}");
    }
    let two = top_peaks(&s.freq_bins, &s.psd_x, 2, 1.0);
    assert_eq!(two.len(), 2);
    assert!(two[0].level > two[1].level);
    assert!((two[0].freq - 40.0).abs() < 2.0);
    assert!((two[1].freq - 43.0).abs() < 2.0);
}

#[test]
fn bins_stop_at_the_range() {
    let s = pushed(&[(60.0, 1.0)], 1.0).spectrum().unwrap();
    assert!(*s.freq_bins.last().unwrap() <= 200.0);
    assert!(*s.freq_bins.last().unwrap() > 195.0);
}

#[test]
fn no_false_peak_after_the_decimator_and_the_spectrum() {
    let fs = 3200.0;
    let n = (fs * 4.0) as usize;
    let t: Vec<f64> = (0..n).map(|i| i as f64 / fs).collect();
    let x: Vec<f64> = t
        .iter()
        .map(|t| (2.0 * PI * 60.0 * t).sin() + (2.0 * PI * 400.0 * t).sin())
        .collect();
    let input = RawSamples {
        t,
        y: x.clone(),
        z: x.clone(),
        x,
    };
    let mut d = Decimator::new(fs, 200.0).unwrap();
    let out = d.process(&input);
    let mut live = LiveSpectrum::new(d.output_rate_hz(), 1.0, 200.0).unwrap();
    let xyz: Vec<f32> = (0..out.len())
        .flat_map(|i| [out.x[i] as f32, out.y[i] as f32, out.z[i] as f32])
        .collect();
    live.push_f32(out.t[0], &xyz).unwrap();
    let s = live.spectrum().unwrap();
    let peaks = top_peaks(&s.freq_bins, &s.psd_sum, 10, 5.0);
    assert!((peaks[0].freq - 60.0).abs() < 2.0, "{peaks:?}");
    for p in &peaks[1..] {
        assert!(p.level < peaks[0].level / 1e4, "{p:?}");
    }
}

#[test]
fn a_burst_in_the_newest_samples_is_in_the_spectrum() {
    // 1535 samples with nfft 1024: whole-buffer segmentation would end 511 samples early.
    let rate = 500.0;
    let n = 1535;
    let burst = 50;
    let mut live = LiveSpectrum::new(rate, 3.07, 200.0).unwrap();
    live.push_f32(0.0, &vec![0.0; (n - burst) * 3]).unwrap();
    let tone = interleaved(&[(100.0, 1.0)], rate, n - burst, burst);
    live.push_f32((n - burst) as f64 / rate, &tone).unwrap();
    let s = live.spectrum().unwrap();
    let peak = s.psd_sum.iter().copied().fold(0.0, f64::max);
    assert!(peak > 0.0, "the newest samples were ignored");
}
