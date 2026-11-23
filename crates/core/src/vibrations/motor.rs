//! The motor profiles: `_compute_motor_profiles` of Shake&Tune's `VibrationsComputation`.

use crate::numeric::{convolve_same, trapz};

/// The PSD of every step on the shared frequency grid: for each measured angle in degrees, the
/// `(speed, psd)` pairs of its steps in the order they were recorded. The order matters, the
/// profile sums the PSDs in it.
pub type PsdByAngleSpeed = Vec<(f64, Vec<(f64, Vec<f64>)>)>;

const PROFILE_SMOOTHING: usize = 20;
/// Power the energy and the area under the profile are raised to for the weights.
const ENERGY_AMPLIFICATION: i32 = 2;

/// The PSD profile of each main angle and the weighted average of both.
///
/// A profile is the mean PSD over the angle's speeds, smoothed with a moving average of 20 bins
/// whose ends are padded with zeros, so it fades towards zero at both ends. The weight of an
/// angle is the square of its energy times the square of the area under its profile. The energy is
/// `angle_energy[angle]`, read with the angle in degrees as the row number of an array that has a
/// row every half degree, so 90 degrees reads the row of 45 degrees. When the weights add up
/// to zero the global profile is the unweighted sum of the profiles.
///
/// Both `main` angles must be in `psds` and below the length of `angle_energy`. The caller checks,
/// as Shake&Tune does. An angle missing from `psds` is skipped.
pub fn motor_profiles(
    freqs: &[f64],
    psds: &PsdByAngleSpeed,
    angle_energy: &[f64],
    main: [f64; 2],
) -> (Vec<(f64, Vec<f64>)>, Vec<f64>) {
    let mut profiles: Vec<(f64, Vec<f64>)> = Vec::with_capacity(main.len());
    let mut weighted_sum = vec![0.0; freqs.len()];
    let mut total_weight = 0.0;
    let kernel = vec![1.0 / PROFILE_SMOOTHING as f64; PROFILE_SMOOTHING];

    for angle in main {
        let Some((_, by_speed)) = psds.iter().find(|(a, _)| *a == angle) else {
            continue;
        };
        let mut sum_curve = vec![0.0; freqs.len()];
        for (_, psd) in by_speed {
            for (total, value) in sum_curve.iter_mut().zip(psd) {
                *total += value;
            }
        }
        let count = by_speed.len() as f64;
        let mean_curve: Vec<f64> = sum_curve.iter().map(|v| v / count).collect();
        let profile = convolve_same(&mean_curve, &kernel);

        let energy = angle_energy[angle as usize].powi(ENERGY_AMPLIFICATION);
        let area = trapz(&profile, freqs).powi(ENERGY_AMPLIFICATION);
        let weight = energy * area;
        for (total, value) in weighted_sum.iter_mut().zip(&profile) {
            *total += value * weight;
        }
        total_weight += weight;
        profiles.push((angle, profile));
    }

    let global = if total_weight != 0.0 {
        weighted_sum.iter().map(|v| v / total_weight).collect()
    } else {
        weighted_sum
    };
    (profiles, global)
}
