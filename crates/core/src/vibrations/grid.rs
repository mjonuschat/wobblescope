//! The direction and speed grid of the vibrations profile and the curves read off it:
//! `_compute_dir_speed_spectrogram`, `_compute_angle_powers`, `_compute_speed_powers` and
//! `_compute_symmetry_analysis` of Shake&Tune's `VibrationsComputation`.

use crate::numeric::{convolve_same, linspace};

/// Energy of every step: for each measured angle in degrees, the `(speed, energy)` pairs of its
/// steps in the order they were recorded.
pub type EnergyByAngleSpeed = Vec<(f64, Vec<(f64, f64)>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kinematics {
    /// The motors are the X and Y axes. Also CoreXZ, whose tested axes are X and Y.
    Cartesian,
    /// The motors drive the diagonals.
    CoreXy,
}

impl Kinematics {
    /// The kinematics Shake&Tune accepts: `cartesian`, `corexz`, `corexy` and their `limited_`
    /// forms. `None` for anything else.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "cartesian" | "limited_cartesian" | "corexz" | "limited_corexz" => {
                Some(Self::Cartesian)
            }
            "corexy" | "limited_corexy" => Some(Self::CoreXy),
            _ => None,
        }
    }

    /// The two angles in degrees that must be measured: the directions of the two motors.
    pub fn main_angles(self) -> [f64; 2] {
        match self {
            Self::Cartesian => [0.0, 90.0],
            Self::CoreXy => [45.0, 135.0],
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    /// 720 angles in degrees from 0 to 360.
    pub angles: Vec<f64>,
    /// `6 * N` speeds in mm/s from the lowest to the highest measured one, for `N` measured speeds.
    pub speeds: Vec<f64>,
    /// One row per angle, one column per speed.
    pub data: Vec<Vec<f64>>,
}

const ANGLE_COUNT: usize = 720;
const SPEEDS_PER_MEASURED: usize = 6;
/// Rows added at each end of the angle powers before smoothing.
const ANGLE_PAD: usize = 9;
const ANGLE_SMOOTHING: usize = 15;
const SPEED_SMOOTHING: usize = 15;

/// Shake&Tune's `get_interpolated_vibrations`: the energy at `speed`, interpolated linearly
/// between the two measured speeds around it, or extrapolated from the first or last pair.
/// `measured` holds the sorted measured speeds, `energy` the energy at each of them.
fn interpolated(measured: &[f64], energy: &[f64], speed: f64) -> f64 {
    let at = measured.partition_point(|s| *s < speed);
    let idx = at.max(1).min(measured.len() - 1);
    let (lower, upper) = (idx.saturating_sub(1), idx);
    energy[lower]
        + (speed - measured[lower]) * (energy[upper] - energy[lower])
            / (measured[upper] - measured[lower])
}

/// Projects the energy measured on the two motor directions onto every direction of travel.
///
/// `energy` must hold both `main` angles with at least two distinct speeds overall (the caller
/// checks, as Shake&Tune does before this step). A speed missing at an angle counts as zero energy,
/// as in Shake&Tune. A speed below the lowest measured one is extrapolated and may give negative
/// energy (D35).
///
/// The angles are `linspace(0, 360, 720)`, which contains both 0 and 360, so the first row equals
/// the last and the step is 0.5007 degrees (D43).
pub fn direction_speed_grid(energy: &EnergyByAngleSpeed, k: Kinematics, main: [f64; 2]) -> Grid {
    let mut measured: Vec<f64> = energy
        .iter()
        .flat_map(|(_, by_speed)| by_speed.iter().map(|(speed, _)| *speed))
        .collect();
    measured.sort_by(f64::total_cmp);
    measured.dedup();
    let by_main = main.map(|angle| {
        measured
            .iter()
            .map(|speed| {
                energy
                    .iter()
                    .find(|(a, _)| *a == angle)
                    .and_then(|(_, by_speed)| by_speed.iter().find(|(s, _)| s == speed))
                    .map_or(0.0, |(_, e)| *e)
            })
            .collect::<Vec<f64>>()
    });

    let angles = linspace(0.0, 360.0, ANGLE_COUNT);
    let speeds = linspace(
        measured[0],
        measured[measured.len() - 1],
        measured.len() * SPEEDS_PER_MEASURED,
    );
    let sqrt_2_inv = 1.0 / 2.0_f64.sqrt();
    let data = angles
        .iter()
        .map(|angle| {
            let radians = angle.to_radians();
            let (sin, cos) = (radians.sin(), radians.cos());
            speeds
                .iter()
                .map(|speed| {
                    let (speed_1, speed_2) = match k {
                        Kinematics::Cartesian => ((speed * cos).abs(), (speed * sin).abs()),
                        Kinematics::CoreXy => (
                            (speed * (cos + sin) * sqrt_2_inv).abs(),
                            (speed * (cos - sin) * sqrt_2_inv).abs(),
                        ),
                    };
                    interpolated(&measured, &by_main[0], speed_1)
                        + interpolated(&measured, &by_main[1], speed_2)
                })
                .collect()
        })
        .collect();
    Grid {
        angles,
        speeds,
        data,
    }
}

/// The energy of every direction: the trapezoid integral of each row over the speed index (unit
/// spacing), smoothed with a moving average of 15 rows.
///
/// For the grid to read as circular, 9 rows from each end are copied to the other before the
/// average and cut off after it. The grid holds both 0 and 360 degrees, so the copies repeat that
/// row (D43).
pub fn angle_powers(grid: &Grid) -> Vec<f64> {
    let powers: Vec<f64> = grid
        .data
        .iter()
        .map(|row| row.windows(2).map(|w| (w[0] + w[1]) / 2.0).sum())
        .collect();
    let n = powers.len();
    let mut extended = powers[n - ANGLE_PAD..].to_vec();
    extended.extend_from_slice(&powers);
    extended.extend_from_slice(&powers[..ANGLE_PAD]);
    let kernel = vec![1.0 / ANGLE_SMOOTHING as f64; ANGLE_SMOOTHING];
    let smoothed = convolve_same(&extended, &kernel);
    smoothed[ANGLE_PAD..smoothed.len() - ANGLE_PAD].to_vec()
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpeedCurves {
    pub min: Vec<f64>,
    pub max: Vec<f64>,
    /// Variance over all directions, rescaled so that its maximum equals the maximum of `max`.
    pub variance: Vec<f64>,
    /// `max` times `variance`, the figure peaks and quiet speeds are read from (D45).
    pub metric: Vec<f64>,
}

/// Replicates the end values `half` times on each side, then averages 15 points (`mode='valid'`).
fn pad_and_smooth(values: &[f64]) -> Vec<f64> {
    let half = SPEED_SMOOTHING / 2;
    let mut padded = vec![values[0]; half];
    padded.extend_from_slice(values);
    padded.extend(std::iter::repeat_n(values[values.len() - 1], half));
    let kernel = 1.0 / SPEED_SMOOTHING as f64;
    padded
        .windows(SPEED_SMOOTHING)
        .map(|w| w.iter().map(|v| v * kernel).sum())
        .collect()
}

/// Minimum, maximum and variance over the directions for every speed, and the metric built from
/// them, each smoothed with a moving average of 15 speeds padded with its end values.
pub fn speed_powers(grid: &Grid) -> SpeedCurves {
    let columns = grid.speeds.len();
    let rows = grid.data.len() as f64;
    let column = |c: usize| grid.data.iter().map(move |row| row[c]);
    let min: Vec<f64> = (0..columns)
        .map(|c| column(c).fold(f64::INFINITY, f64::min))
        .collect();
    let max: Vec<f64> = (0..columns)
        .map(|c| column(c).fold(f64::NEG_INFINITY, f64::max))
        .collect();
    let variance: Vec<f64> = (0..columns)
        .map(|c| {
            let mean = column(c).sum::<f64>() / rows;
            column(c).map(|v| (v - mean) * (v - mean)).sum::<f64>() / rows
        })
        .collect();
    let max_of = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (variance_max, max_max) = (max_of(&variance), max_of(&max));
    let variance: Vec<f64> = variance
        .iter()
        .map(|v| v / variance_max * max_max)
        .collect();
    let metric: Vec<f64> = max.iter().zip(&variance).map(|(m, v)| m * v).collect();
    SpeedCurves {
        min: pad_and_smooth(&min),
        max: pad_and_smooth(&max),
        variance: pad_and_smooth(&variance),
        metric: pad_and_smooth(&metric),
    }
}

/// `numpy.corrcoef(x, y)[0, 1]`.
fn corrcoef(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let (mean_x, mean_y) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for (a, b) in x.iter().zip(y) {
        let (da, db) = (a - mean_x, b - mean_y);
        sxx += da * da;
        syy += db * db;
        sxy += da * db;
    }
    let (cxx, cyy, cxy) = (sxx / (n - 1.0), syy / (n - 1.0), sxy / (n - 1.0));
    let r = cxy / cxx.sqrt() / cyy.sqrt();
    if r.is_nan() { r } else { r.clamp(-1.0, 1.0) }
}

/// How alike the grid looks on the two sides of the midpoint of the two motor angles, as a score
/// of 10 to 100: `100 * correlation^0.75 + 10`.
///
/// The two sections are the quarter turn (180 rows) before the midpoint and the one after it, in
/// the same order, so one is the other rotated by 90 degrees and not mirrored (D37). The score is
/// capped at 100 but never raised to 0, because Shake&Tune calls `np.clip(0, 100, x)` with its
/// arguments in the wrong order. An anti-correlated grid raises a negative number to a fractional
/// power and gives NaN (D36). Both are reproduced.
pub fn symmetry(grid: &Grid, main: [f64; 2]) -> f64 {
    let total = grid.angles.len();
    let half = total / 2;
    let extended: Vec<&Vec<f64>> = grid.data[total - half..].iter().chain(&grid.data).collect();
    let midpoint = (main[0] + main[1]) / 2.0;
    let split = (midpoint * (total as f64 / 360.0) + half as f64) as usize;
    let segment = half / 2;
    let flatten = |rows: &[&Vec<f64>]| -> Vec<f64> {
        rows.iter().flat_map(|row| row.iter().copied()).collect()
    };
    let first = flatten(&extended[split - segment..split]);
    let second = flatten(&extended[split..split + segment]);
    let biased = 100.0 * corrcoef(&first, &second).powf(0.75) + 10.0;
    if biased.is_nan() {
        biased
    } else {
        biased.min(100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetry_of_anticorrelated_grid_is_not_nan() {
        // Section one is 0..180 down the rows, section two is its negation: correlation -1.
        let mut data = vec![vec![0.0]; ANGLE_COUNT];
        for k in 0..180 {
            data[(630 + k) % ANGLE_COUNT][0] = k as f64;
            data[90 + k][0] = -(k as f64);
        }
        let grid = Grid {
            angles: linspace(0.0, 360.0, ANGLE_COUNT),
            speeds: vec![1.0],
            data,
        };
        // Shake&Tune returns NaN here (D36), and so does the port.
        assert!(symmetry(&grid, [0.0, 90.0]).is_nan());
    }

    #[test]
    fn kinematics_names_map_as_shaketune_maps_them() {
        assert_eq!(
            Kinematics::parse("limited_corexz"),
            Some(Kinematics::Cartesian)
        );
        assert_eq!(
            Kinematics::parse("corexy").unwrap().main_angles(),
            [45.0, 135.0]
        );
        assert_eq!(Kinematics::parse("delta"), None);
    }
}
