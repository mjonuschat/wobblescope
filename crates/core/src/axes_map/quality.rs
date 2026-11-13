//! Quality assessment of an axes-map result: a port of Shake&Tune's `_validate_results`.

use super::{NoiseVerdict, Quality, QualityStatus};

/// Lowest acceptable mean detection confidence.
const MIN_CONFIDENCE: f64 = 0.5;
/// Largest acceptable angle error (degrees).
const MAX_ANGLE_ERROR: f64 = 15.0;
/// Standard gravity in mm/s^2.
const EXPECTED_GRAVITY: f64 = 9810.0;
/// Allowed relative deviation from `EXPECTED_GRAVITY`.
const GRAVITY_TOLERANCE: f64 = 0.20;
/// Noise level (mm/s^2) up to which the verdict is `Fine`.
const NOISE_FINE_MAX: f64 = 350.0;
/// Noise level (mm/s^2) up to which the verdict is `High`.
const NOISE_HIGH_MAX: f64 = 700.0;

/// Index of the largest absolute component; the first wins ties, like numpy's `argmax`.
pub(crate) fn dominant_index(v: &[f64; 3]) -> usize {
    let mut best = 0;
    for k in 1..3 {
        if v[k].abs() > v[best].abs() {
            best = k;
        }
    }
    best
}

/// Check the detection against the reference's thresholds.
///
/// `gravity` and `noise` are in mm/s^2, `accel` is the commanded acceleration in mm/s^2.
pub(crate) fn validate(
    directions: &[[f64; 3]; 3],
    confidences: [f64; 3],
    angle_errors: [f64; 3],
    noise: f64,
    gravity: f64,
    accel: f64,
) -> Quality {
    let mut messages = Vec::new();
    let mut status = QualityStatus::Ok;

    let detected = directions.map(|d| dominant_index(&d));
    if detected[0] == detected[1] || detected[0] == detected[2] || detected[1] == detected[2] {
        status = QualityStatus::Error;
        messages.push("Same accelerometer axis detected for multiple machine axes!".to_owned());
    }

    let mean_confidence = confidences.iter().sum::<f64>() / 3.0;
    if mean_confidence < MIN_CONFIDENCE {
        warn(&mut status);
        messages.push(format!(
            "Low detection confidence ({:.0}%)",
            mean_confidence * 100.0
        ));
    }

    let max_angle = angle_errors
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    if max_angle > MAX_ANGLE_ERROR {
        warn(&mut status);
        messages.push(format!(
            "High angle error detected ({max_angle:.1} degrees)"
        ));
    }

    let low = EXPECTED_GRAVITY * (1.0 - GRAVITY_TOLERANCE);
    let high = EXPECTED_GRAVITY * (1.0 + GRAVITY_TOLERANCE);
    if !(low <= gravity && gravity <= high) {
        warn(&mut status);
        messages.push(format!(
            "Unusual gravity reading ({:.2} m/s^2)",
            gravity / 1000.0
        ));
    }

    if accel != 0.0 && noise > accel * 0.3 {
        warn(&mut status);
        messages.push(format!("High noise level ({noise:.0} mm/s^2)"));
    }

    if status == QualityStatus::Ok {
        messages.push("Detection quality: GOOD".to_owned());
    }
    Quality { status, messages }
}

fn warn(status: &mut QualityStatus) {
    if *status == QualityStatus::Ok {
        *status = QualityStatus::Warning;
    }
}

/// Classify a noise level in mm/s^2.
pub(crate) fn noise_verdict(noise: f64) -> NoiseVerdict {
    if noise <= NOISE_FINE_MAX {
        NoiseVerdict::Fine
    } else if noise <= NOISE_HIGH_MAX {
        NoiseVerdict::High
    } else {
        NoiseVerdict::TooHigh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    #[test]
    fn a_clean_result_is_good() {
        let q = validate(&IDENTITY, [1.0; 3], [1.0; 3], 100.0, 9810.0, 1500.0);
        assert_eq!(q.status, QualityStatus::Ok);
        assert_eq!(q.messages, ["Detection quality: GOOD"]);
    }

    #[test]
    fn a_repeated_chip_axis_is_an_error() {
        let mut d = IDENTITY;
        d[1] = d[0];
        let q = validate(&d, [1.0; 3], [0.0; 3], 100.0, 9810.0, 1500.0);
        assert_eq!(q.status, QualityStatus::Error);
        assert_eq!(
            q.messages,
            ["Same accelerometer axis detected for multiple machine axes!"]
        );
    }

    #[test]
    fn warnings_use_the_reference_texts() {
        let q = validate(&IDENTITY, [0.2; 3], [16.04; 3], 600.0, 5000.0, 1500.0);
        assert_eq!(q.status, QualityStatus::Warning);
        assert_eq!(
            q.messages,
            [
                "Low detection confidence (20%)",
                "High angle error detected (16.0 degrees)",
                "Unusual gravity reading (5.00 m/s^2)",
                "High noise level (600 mm/s^2)",
            ]
        );
    }

    #[test]
    fn noise_verdict_thresholds() {
        assert_eq!(noise_verdict(350.0), NoiseVerdict::Fine);
        assert_eq!(noise_verdict(350.1), NoiseVerdict::High);
        assert_eq!(noise_verdict(700.0), NoiseVerdict::High);
        assert_eq!(noise_verdict(700.1), NoiseVerdict::TooHigh);
    }
}
