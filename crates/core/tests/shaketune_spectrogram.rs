#[macro_use]
mod common;

use common::{assert_close, close, load_csv, shaketune};
use wobblescope_core::eligibility::{Coverage, DatasetInput, Health};
use wobblescope_core::spectrogram::{percentile, spectrogram, spectrogram_of};
use wobblescope_core::{CoreError, RawSamples};

fn chip_a() -> RawSamples {
    load_csv("chip_a")
}

parity_test! {
    fn matches_shaketune_on_two_recordings() {
        let oracle = shaketune();
        assert_eq!(oracle.spectrograms.len(), 2);
        for (name, want) in &oracle.spectrograms {
            let got = spectrogram(&load_csv(name), 200.0).unwrap();
            assert_eq!(got.times.len(), want.n_segments, "{name}: segments");
            assert_eq!(got.power.len(), want.n_segments, "{name}: rows");
            assert_close(&format!("{name} times"), &got.times, &want.times);
            assert_close(&format!("{name} freq_bins"), &got.freq_bins, &want.freq_bins);
            for (i, row) in want.rows.iter().enumerate() {
                assert_close(&format!("{name} row {i}"), &got.power[i], row);
            }
            close(&format!("{name} floor"), got.floor, want.floor);
            close(&format!("{name} max"), got.max, want.max);
        }
    }
}

#[test]
fn a_too_short_recording_is_refused() {
    let short = chip_a().slice(0..1000);
    assert!(matches!(
        spectrogram(&short, 200.0),
        Err(CoreError::TooFewSamples { .. })
    ));
}

#[test]
fn mismatched_columns_and_bad_timestamps_are_refused() {
    let mut columns = chip_a();
    columns.x.pop();
    assert_eq!(
        spectrogram(&columns, 200.0),
        Err(CoreError::MismatchedColumns)
    );
    let mut times = chip_a();
    times.t[10] = times.t[9];
    assert_eq!(
        spectrogram(&times, 200.0),
        Err(CoreError::InvalidTimestamps)
    );
}

#[test]
fn a_non_finite_sample_is_refused() {
    let mut samples = chip_a();
    samples.y[5] = f64::NAN;
    assert_eq!(
        spectrogram(&samples, 200.0),
        Err(CoreError::NonFiniteSample)
    );
}

#[test]
fn the_percentile_interpolates_like_numpy() {
    let hundred: Vec<f64> = (0..=100).map(f64::from).collect();
    assert_eq!(percentile(&hundred, 5.0), Some(5.0));
    let got = percentile(&[2.0, 1.0], 5.0).unwrap();
    assert!((got - 1.05).abs() < 1e-12, "{got}");
    assert_eq!(percentile(&[], 5.0), None);
}

fn input(window: Option<std::ops::Range<f64>>) -> DatasetInput {
    DatasetInput {
        chip: "chip_a".to_owned(),
        point: 0,
        samples: chip_a(),
        health: Health::Ok,
        coverage: Coverage::Complete,
        include_override: false,
        window,
        exclusions: Vec::new(),
        load_error: None,
    }
}

#[test]
fn without_a_window_the_input_maps_the_whole_recording() {
    assert_eq!(
        spectrogram_of(&input(None), 200.0).unwrap(),
        spectrogram(&chip_a(), 200.0).unwrap()
    );
}

#[test]
fn a_window_maps_only_the_window() {
    let start = chip_a().t[0];
    let whole = spectrogram_of(&input(None), 200.0).unwrap();
    let cut = spectrogram_of(&input(Some(start + 1.0..start + 4.0)), 200.0).unwrap();
    assert!(cut.power.len() < whole.power.len());
    assert!(*cut.times.last().unwrap() < 3.0);
}

#[test]
fn a_recording_that_failed_to_load_returns_its_error() {
    let mut failed = input(None);
    failed.load_error = Some("c0-s0.zst is damaged".to_owned());
    match spectrogram_of(&failed, 200.0) {
        Err(CoreError::InvalidParameter(message)) => assert!(message.contains("damaged")),
        other => panic!("expected InvalidParameter, got {other:?}"),
    }
}
