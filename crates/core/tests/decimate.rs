//! Tests for the streaming decimator.

use std::f64::consts::PI;

use wobblescope_core::decimate::Decimator;
use wobblescope_core::{CoreError, RawSamples};

const FS: f64 = 3200.0;

fn tone(freq: f64, seconds: f64) -> RawSamples {
    let n = (FS * seconds) as usize;
    let t: Vec<f64> = (0..n).map(|i| i as f64 / FS).collect();
    let x: Vec<f64> = t.iter().map(|t| (2.0 * PI * freq * t).sin()).collect();
    RawSamples {
        t,
        y: x.clone(),
        z: x.clone(),
        x,
    }
}

fn rms(x: &[f64]) -> f64 {
    (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
}

/// RMS of a tone after the decimator, discarding the first 0.25 s.
fn rms_ratio(freq: f64) -> f64 {
    let input = tone(freq, 4.0);
    let mut d = Decimator::new(FS, 200.0).unwrap();
    let out = d.process(&input);
    let skip = (0.25 * d.output_rate_hz()) as usize;
    let skip_in = (0.25 * FS) as usize;
    rms(&out.x[skip..]) / rms(&input.x[skip_in..])
}

#[test]
fn factor_is_six_for_3200_hz_and_200_hz_range() {
    let d = Decimator::new(3200.0, 200.0).unwrap();
    assert_eq!(d.factor(), 6);
    assert!((d.output_rate_hz() - 3200.0 / 6.0).abs() < 0.01);
}

#[test]
fn factor_is_one_when_the_range_needs_the_whole_rate() {
    let mut d = Decimator::new(3200.0, 1250.0).unwrap();
    assert_eq!(d.factor(), 1);
    let input = tone(100.0, 0.1);
    assert_eq!(d.process(&input), input);
}

#[test]
fn tones_inside_the_range_pass_within_1_db() {
    for f in [50.0, 200.0] {
        let r = rms_ratio(f);
        assert!((r - 1.0).abs() < 0.12, "{f} Hz ratio {r}");
    }
}

#[test]
fn tones_that_would_fold_into_the_range_are_cut_by_at_least_50_db() {
    for f in [333.4, 400.0, 600.0] {
        let r = rms_ratio(f);
        assert!(r < 0.0032, "{f} Hz ratio {r}");
    }
}

#[test]
fn chunking_does_not_change_the_output() {
    let n = (2.0 * FS) as usize;
    let mut seed = 12345u64;
    let mut noise = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5
    };
    let input = RawSamples {
        t: (0..n).map(|i| i as f64 / FS).collect(),
        x: (0..n).map(|_| noise()).collect(),
        y: (0..n).map(|_| noise()).collect(),
        z: (0..n).map(|_| noise()).collect(),
    };
    let whole = Decimator::new(FS, 200.0).unwrap().process(&input);
    for chunk in [1, 7, 4096] {
        let mut d = Decimator::new(FS, 200.0).unwrap();
        let mut joined = RawSamples {
            t: vec![],
            x: vec![],
            y: vec![],
            z: vec![],
        };
        for start in (0..n).step_by(chunk) {
            let end = (start + chunk).min(n);
            let part = d.process(&RawSamples {
                t: input.t[start..end].to_vec(),
                x: input.x[start..end].to_vec(),
                y: input.y[start..end].to_vec(),
                z: input.z[start..end].to_vec(),
            });
            joined.t.extend(part.t);
            joined.x.extend(part.x);
            joined.y.extend(part.y);
            joined.z.extend(part.z);
        }
        assert_eq!(joined, whole, "chunk size {chunk}");
    }
}

#[test]
fn bad_parameters_are_refused() {
    for (rate, max) in [(0.0, 200.0), (3200.0, 0.0), (f64::NAN, 200.0)] {
        assert!(matches!(
            Decimator::new(rate, max),
            Err(CoreError::InvalidParameter(_))
        ));
    }
}

#[test]
fn reset_makes_the_output_equal_to_a_fresh_decimators() {
    let mut used = Decimator::new(FS, 200.0).unwrap();
    let mut step = tone(100.0, 0.5);
    step.x.iter_mut().for_each(|v| *v += 50.0);
    used.process(&step);
    // Odd length so the phase is not back at zero by chance.
    used.process(&tone(40.0, 0.1234));
    used.reset();

    let input = tone(60.0, 1.0);
    let mut fresh = Decimator::new(FS, 200.0).unwrap();
    assert_eq!(used.process(&input), fresh.process(&input));
}
