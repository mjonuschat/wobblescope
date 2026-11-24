//! One chip's vibrations profile: `VibrationsComputation.compute` of Shake&Tune, from the steps
//! to every figure of the report.

use serde::{Deserialize, Serialize};

use super::correct::{Measured, corrected_grid};
use super::grid::{
    EnergyByAngleSpeed, Grid, Kinematics, Model, angle_powers, direction_speed_grid, speed_powers,
    symmetry,
};
use super::motor::{PsdByAngleSpeed, motor_profiles};
use super::steps::{StepInput, step_psds};
use super::zones::{Zone, filter_and_split_zones, low_energy_zones};
use crate::CORE_VERSION;
use crate::error::CoreError;
use crate::peaks::detect_peaks_relative;
use crate::resonance::estimate_damping_above;

/// Lowest frequency the motor resonance is looked for at, in Hz.
const MOTOR_MIN_FREQ: f64 = 30.0;
/// Speeds below this fraction of the way to the highest vibration metric are quiet (threshold of
/// `identify_low_energy_zones` for good speeds).
const GOOD_SPEEDS_THRESHOLD: f64 = 0.7;
/// Threshold of `identify_low_energy_zones` for good angles.
const GOOD_ANGLES_THRESHOLD: f64 = 1.1;
/// Width in mm/s cut out of a good speed zone either side of a peak.
const PEAK_CLEARANCE_MM_S: f64 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VibrationsParams {
    /// Highest frequency analysed, in Hz.
    pub max_freq: f64,
    pub kinematics: Kinematics,
}

impl Default for VibrationsParams {
    fn default() -> Self {
        Self {
            max_freq: 1000.0,
            kinematics: Kinematics::Cartesian,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct StepEnergy {
    /// Movement angle in degrees.
    pub angle: f64,
    /// Movement speed in mm/s.
    pub speed: f64,
    /// Trapezoid integral of the step's summed PSD up to the maximum frequency.
    pub energy: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
/// The energy recorded at one measured direction, drawn as it was recorded and not read from the grid.
pub struct MeasuredCurve {
    /// Direction in degrees.
    pub angle: f64,
    /// Recorded speeds in mm/s, ascending.
    pub speeds: Vec<f64>,
    /// Energy at each of `speeds`, in the same order.
    pub energy: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MotorResult {
    /// Frequencies in Hz.
    pub freqs: Vec<f64>,
    /// The profile of each main angle, keyed by the angle in degrees.
    pub profiles: Vec<(f64, Vec<f64>)>,
    /// The weighted average of the profiles.
    pub combined: Vec<f64>,
    /// Resonance frequency in Hz.
    pub fr: f64,
    /// Damping ratio. `None` when the half-power points cannot be found.
    pub zeta: Option<f64>,
    pub peak_index: usize,
    /// The strongest bin of the profile lies below the resonance search range.
    pub low_freq_warning: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct VibrationsResult {
    /// Energy of every step, empty for a worst case.
    pub steps: Vec<StepEnergy>,
    /// Grid speeds in mm/s.
    pub speeds: Vec<f64>,
    /// Grid angles in degrees.
    pub angles: Vec<f64>,
    pub angle_energy: Vec<f64>,
    /// Energy over angle (rows) and speed (columns).
    pub grid: Vec<Vec<f64>>,
    pub speed_min: Vec<f64>,
    pub speed_max: Vec<f64>,
    pub speed_variance: Vec<f64>,
    pub vibration_metric: Vec<f64>,
    /// Indices into `speeds` of the vibration peaks.
    pub peaks: Vec<usize>,
    /// Speeds of the peaks, the bad speeds.
    pub peak_speeds: Vec<f64>,
    pub good_speeds: Vec<Zone>,
    pub good_angles: Vec<Zone>,
    /// Symmetry score. `None` when Shake&Tune's value is NaN.
    pub symmetry: Option<f64>,
    /// The motor profile. `None` for a worst case and when no resonance can be found.
    pub motor: Option<MotorResult>,
    /// The recorded curve of every angle that had steps, ascending by angle. For a worst case, the
    /// angles every chip measured at the same speeds, each the element-wise maximum.
    pub measured: Vec<MeasuredCurve>,
    /// `wobblescope-core` version that produced it.
    pub core_version: String,
}

/// The figures derived from a grid: everything but the steps and the motor profile.
///
/// The grid must have at least two speeds, as the speed step is read from the first two. Both
/// callers check this before they call.
pub(crate) fn derive(grid: &Grid, kinematics: Kinematics) -> VibrationsResult {
    debug_assert!(grid.speeds.len() >= 2, "callers check for two speeds");
    let main = kinematics.main_angles();
    let angle_energy = angle_powers(grid);
    let curves = speed_powers(grid);

    // The window and the vicinity are counts of grid points, so their width in mm/s depends
    // on the grid's speed step. Kept as Shake&Tune has it.
    let max_metric = curves
        .metric
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let peaks = detect_peaks_relative(&curves.metric, 0.05 * max_metric, 0.04, 10, 10);
    let peak_speeds: Vec<f64> = peaks.iter().map(|i| grid.speeds[*i]).collect();

    // Shake&Tune pairs each distinct peak speed with one index.
    let mut peak_speed_indices: Vec<(f64, usize)> = Vec::new();
    for (speed, index) in peak_speeds.iter().zip(&peaks) {
        if !peak_speed_indices.iter().any(|(s, _)| s == speed) {
            peak_speed_indices.push((*speed, *index));
        }
    }
    let deletion_range = (PEAK_CLEARANCE_MM_S / (grid.speeds[1] - grid.speeds[0])) as usize;
    let speed_zones = low_energy_zones(&curves.metric, GOOD_SPEEDS_THRESHOLD);
    let good_speeds = filter_and_split_zones(
        &grid.speeds,
        &speed_zones,
        &peak_speed_indices,
        deletion_range,
    );
    let good_angles = low_energy_zones(&angle_energy, GOOD_ANGLES_THRESHOLD);
    let symmetry = Some(symmetry(grid, main)).filter(|s| !s.is_nan());

    VibrationsResult {
        steps: Vec::new(),
        speeds: grid.speeds.clone(),
        angles: grid.angles.clone(),
        angle_energy,
        grid: grid.data.clone(),
        speed_min: curves.min,
        speed_max: curves.max,
        speed_variance: curves.variance,
        vibration_metric: curves.metric,
        peaks,
        peak_speeds,
        good_speeds,
        good_angles,
        symmetry,
        motor: None,
        measured: Vec::new(),
        core_version: CORE_VERSION.to_string(),
    }
}

pub(super) fn measured_curves(energy: &EnergyByAngleSpeed) -> Vec<Measured> {
    let mut curves: Vec<Measured> = energy
        .iter()
        .map(|(angle, by_speed)| {
            let mut by_speed = by_speed.clone();
            by_speed.sort_by(|a, b| a.0.total_cmp(&b.0));
            Measured {
                angle: *angle,
                speeds: by_speed.iter().map(|(s, _)| *s).collect(),
                energy: by_speed.iter().map(|(_, e)| *e).collect(),
            }
        })
        .collect();
    curves.sort_by(|a, b| a.angle.total_cmp(&b.angle));
    curves
}

/// Fails with "no usable vibration energy" when every column of a corrected grid is constant,
/// which leaves the speed curves undefined. Columns are compared cell by cell, because the
/// variance of identical cells is only zero up to rounding.
fn check_usable_grid(grid: &Grid) -> Result<(), CoreError> {
    let varies = (0..grid.speeds.len()).any(|c| {
        let first = grid.data[0][c];
        grid.data.iter().any(|row| row[c] != first)
    });
    if varies {
        Ok(())
    } else {
        Err(CoreError::InvalidParameter(
            "no usable vibration energy".into(),
        ))
    }
}

/// Fails with "no usable vibration energy" when a figure derived from a corrected grid is not
/// finite. A NaN symmetry is not one: it is already `None`.
fn check_usable_figures(result: &VibrationsResult) -> Result<(), CoreError> {
    let finite = |v: &[f64]| v.iter().all(|x| x.is_finite());
    if finite(&result.speed_min)
        && finite(&result.speed_max)
        && finite(&result.speed_variance)
        && finite(&result.vibration_metric)
        && finite(&result.peak_speeds)
        && finite(&result.angle_energy)
    {
        Ok(())
    } else {
        Err(CoreError::InvalidParameter(
            "no usable vibration energy".into(),
        ))
    }
}

/// The vibrations profile of one chip.
///
/// Steps may be at any angle. The main angles of the kinematics are required and give the model
/// and the motor profile. Every other angle is an extra: it must have the speeds of the motor
/// angles, and when there is one the angles are drawn from the model corrected by the extras
/// instead of the model. With none, the result is the model's, as before.
///
/// # Errors
/// [`CoreError::InvalidParameter`] when there are no steps, when a main angle of the kinematics
/// has no step, or when the steps cover fewer than two distinct speeds. It also returns what
/// [`CalibrationData::from_samples`](crate::psd::CalibrationData::from_samples) rejects in a
/// recording, such as one with no samples. With extras it also rejects an extra at other speeds
/// than the motor angles, and a corrected grid that has no variance or no finite figures.
pub fn analyze_chip(
    steps: &[StepInput],
    p: &VibrationsParams,
) -> Result<VibrationsResult, CoreError> {
    let (freqs, step_psds) = step_psds(steps, p.max_freq)?;
    let main = p.kinematics.main_angles();
    for angle in main {
        if !step_psds.iter().any(|s| s.angle == angle) {
            return Err(CoreError::InvalidParameter(format!(
                "the vibrations profile needs steps at {angle} degrees"
            )));
        }
    }
    let mut distinct: Vec<f64> = step_psds.iter().map(|s| s.speed).collect();
    distinct.sort_by(f64::total_cmp);
    distinct.dedup();
    if distinct.len() < 2 {
        return Err(CoreError::InvalidParameter(
            "the vibrations profile needs at least two distinct speeds".into(),
        ));
    }

    let mut energy: EnergyByAngleSpeed = Vec::new();
    let mut psds: PsdByAngleSpeed = Vec::new();
    for step in &step_psds {
        match energy.iter_mut().find(|(a, _)| *a == step.angle) {
            Some((_, by_speed)) => by_speed.push((step.speed, step.energy)),
            None => energy.push((step.angle, vec![(step.speed, step.energy)])),
        }
        match psds.iter_mut().find(|(a, _)| *a == step.angle) {
            Some((_, by_speed)) => by_speed.push((step.speed, step.psd.clone())),
            None => psds.push((step.angle, vec![(step.speed, step.psd.clone())])),
        }
    }

    let curves = measured_curves(&energy);
    let (motor, extras): (Vec<Measured>, Vec<Measured>) = curves
        .iter()
        .cloned()
        .partition(|m| main.contains(&m.angle));
    let motor_energy: EnergyByAngleSpeed = energy
        .iter()
        .filter(|(angle, _)| main.contains(angle))
        .cloned()
        .collect();

    let model_grid = direction_speed_grid(&motor_energy, p.kinematics, main);
    let mut result = if extras.is_empty() {
        derive(&model_grid, p.kinematics)
    } else {
        let model = Model::new(&motor_energy, p.kinematics, main);
        let corrected = corrected_grid(&model, &extras, &motor)?;
        check_usable_grid(&corrected)?;
        let corrected_result = derive(&corrected, p.kinematics);
        check_usable_figures(&corrected_result)?;
        corrected_result
    };
    // The motor profile is weighted by the model grid's angular energy, extras or not.
    let model_angle_energy = angle_powers(&model_grid);
    let (profiles, combined) = motor_profiles(&freqs, &psds, &model_angle_energy, main);
    result.motor = estimate_damping_above(&freqs, &combined, MOTOR_MIN_FREQ).map(|m| MotorResult {
        freqs,
        profiles,
        combined,
        fr: m.fr,
        zeta: m.zeta,
        peak_index: m.peak_index,
        low_freq_warning: m.low_freq_max,
    });
    result.measured = curves
        .into_iter()
        .map(|m| MeasuredCurve {
            angle: m.angle,
            speeds: m.speeds,
            energy: m.energy,
        })
        .collect();
    result.steps = step_psds
        .iter()
        .map(|s| StepEnergy {
            angle: s.angle,
            speed: s.speed,
            energy: s.energy,
        })
        .collect();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::numeric::linspace;

    fn usable() -> VibrationsResult {
        let speeds = linspace(20.0, 60.0, 12);
        let angles = linspace(0.0, 360.0, 720);
        let data = (0..720)
            .map(|a| {
                (0..12)
                    .map(|s| 1.0 + ((a as f64 * 0.05 + s as f64).sin() + 1.0))
                    .collect()
            })
            .collect();
        derive(
            &Grid {
                angles,
                speeds,
                data,
            },
            Kinematics::Cartesian,
        )
    }

    #[test]
    fn a_non_finite_speed_curve_peak_or_angular_energy_fails_the_chip() {
        assert!(check_usable_figures(&usable()).is_ok());
        let mut curve = usable();
        curve.speed_max[3] = f64::INFINITY;
        let mut peak = usable();
        peak.peak_speeds.push(f64::NAN);
        let mut angular = usable();
        angular.angle_energy[7] = f64::NAN;
        for result in [curve, peak, angular] {
            let err = check_usable_figures(&result).unwrap_err();
            assert!(
                err.to_string().contains("no usable vibration energy"),
                "{err}"
            );
        }
    }
}
