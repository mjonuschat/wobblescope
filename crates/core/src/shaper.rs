//! Built-in input shapers as impulse sequences, ported from Klipper's `shaper_defs.py`.

use std::f64::consts::PI;
use std::sync::OnceLock;

use crate::error::CoreError;

/// Remaining vibration the shaper is designed for, as a reduction factor.
pub const SHAPER_VIBRATION_REDUCTION: f64 = 20.0;
/// Damping ratio used unless the caller overrides it.
pub const DEFAULT_DAMPING_RATIO: f64 = 0.1;
/// Shapers tried by automatic tuning, in evaluation order.
pub const AUTOTUNE_SHAPERS: [&str; 5] = ["zv", "mzv", "ei", "2hump_ei", "3hump_ei"];

/// A shaper as (amplitude, time) impulses.
#[derive(Debug, Clone, PartialEq)]
pub struct Impulses {
    /// Impulse amplitudes.
    pub a: Vec<f64>,
    /// Impulse times in seconds.
    pub t: Vec<f64>,
}

/// An input shaper family: given a frequency and damping ratio it yields an impulse sequence.
pub trait Shaper: Sync {
    /// Lowercase identifier, as used in Klipper configuration.
    fn name(&self) -> &'static str;
    /// Lowest frequency the autotuner searches, chosen for a projected `max_accel` near 1500.
    fn min_freq(&self) -> f64;
    /// Builds the impulse sequence.
    ///
    /// # Errors
    /// The frequency is not positive and finite, or the damping ratio is outside the shaper's
    /// supported range.
    fn impulses(&self, freq: f64, damping_ratio: f64) -> Result<Impulses, CoreError>;
}

type Built = Result<(Vec<f64>, Vec<f64>), CoreError>;

struct Def {
    name: &'static str,
    min_freq: f64,
    max_damping: f64,
    build: fn(f64, f64) -> Built,
}

impl Shaper for Def {
    fn name(&self) -> &'static str {
        self.name
    }

    fn min_freq(&self) -> f64 {
        self.min_freq
    }

    fn impulses(&self, freq: f64, damping_ratio: f64) -> Result<Impulses, CoreError> {
        if !(freq.is_finite() && freq > 0.0) {
            return Err(CoreError::ShaperInit(format!("invalid frequency {freq}")));
        }
        if !(0.0..self.max_damping).contains(&damping_ratio) {
            return Err(CoreError::ShaperInit(format!(
                "damping ratio {damping_ratio} outside [0, {}) for {}",
                self.max_damping, self.name
            )));
        }
        let (a, t) = (self.build)(freq, damping_ratio)?;
        Ok(Impulses { a, t })
    }
}

fn zv(freq: f64, dr: f64) -> Built {
    let df = (1.0 - dr * dr).sqrt();
    let k = (-dr * PI / df).exp();
    let t_d = 1.0 / (freq * df);
    Ok((vec![1.0, k], vec![0.0, 0.5 * t_d]))
}

fn zvd(freq: f64, dr: f64) -> Built {
    let df = (1.0 - dr * dr).sqrt();
    let k = (-dr * PI / df).exp();
    let t_d = 1.0 / (freq * df);
    Ok((vec![1.0, 2.0 * k, k * k], vec![0.0, 0.5 * t_d, t_d]))
}

/// Solves the square system `m x = rhs` by Gaussian elimination with partial pivoting.
fn solve(mut m: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Option<Vec<f64>> {
    let n = rhs.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| m[i][col].abs().total_cmp(&m[j][col].abs()))?;
        if m[pivot][col].abs() < 1e-10 {
            return None;
        }
        m.swap(col, pivot);
        rhs.swap(col, pivot);
        for row in col + 1..n {
            let f = m[row][col] / m[col][col];
            let pivot_row = m[col].clone();
            for (dst, src) in m[row].iter_mut().zip(&pivot_row).skip(col) {
                *dst -= f * src;
            }
            rhs[row] -= f * rhs[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let tail: f64 = (row + 1..n).map(|k| m[row][k] * x[k]).sum();
        x[row] = (rhs[row] - tail) / m[row][row];
    }
    Some(x)
}

/// Undamped MZV amplitudes and normalised times (`get_mzv_coeffs`): the least-squares solution
/// of "amplitudes sum to one and the shaper has zeros at the designed frequencies".
fn mzv_coeffs(n: usize, t: f64) -> Result<(Vec<f64>, Vec<f64>), CoreError> {
    let nf = n as f64;
    let tau = t * (nf - 2.0) / (nf - 2.0 * t - 1.0);
    let times: Vec<f64> = (0..n).map(|i| i as f64 * t / (nf - 1.0)).collect();
    let mut rows = vec![vec![1.0; n]];
    let mut f = vec![1.0];
    for i in 0..n - 1 {
        let w: Vec<f64> = times
            .iter()
            .map(|tj| 2.0 * PI * (1.0 + i as f64 / tau) * tj)
            .collect();
        rows.push(w.iter().map(|v| v.cos()).collect());
        rows.push(w.iter().map(|v| v.sin()).collect());
        f.extend([0.0, 0.0]);
    }
    let mtm: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| rows.iter().map(|r| r[i] * r[j]).sum())
                .collect()
        })
        .collect();
    let mtf: Vec<f64> = (0..n)
        .map(|i| rows.iter().zip(&f).map(|(r, f)| r[i] * f).sum())
        .collect();
    let a = solve(mtm, mtf)
        .ok_or_else(|| CoreError::ShaperInit(format!("ill-formed shaper with n={n}, t={t}")))?;
    if a.iter().any(|&v| v < -0.00001) {
        return Err(CoreError::ShaperInit(format!(
            "negative-valued shaper with n={n}, t={t}"
        )));
    }
    Ok((a, times))
}

const MZV_N: usize = 3;
const MZV_T: f64 = 0.75;

/// The MZV coefficients for [`MZV_N`] and [`MZV_T`], the only values the shaper uses, solved on
/// first use.
fn mzv_base() -> Result<(Vec<f64>, Vec<f64>), CoreError> {
    static BASE: OnceLock<Option<(Vec<f64>, Vec<f64>)>> = OnceLock::new();
    BASE.get_or_init(|| mzv_coeffs(MZV_N, MZV_T).ok())
        .clone()
        .ok_or_else(|| {
            CoreError::ShaperInit(format!("ill-formed shaper with n={MZV_N}, t={MZV_T}"))
        })
}

fn mzv(freq: f64, dr: f64) -> Built {
    let (n, t) = (MZV_N, MZV_T);
    let (mut a, mut times) = mzv_base()?;
    let nf = n as f64;
    let df = (1.0 - dr * dr).sqrt();
    let k = (-2.0 * t * dr * PI / ((nf - 1.0) * df)).exp();
    let t_d = 1.0 / (freq * df);
    let mut kp = k;
    for i in 1..n {
        times[i] *= t_d;
        a[i] *= kp;
        kp *= k;
    }
    Ok((a, times))
}

fn ei(freq: f64, dr: f64) -> Built {
    let v = 1.0 / SHAPER_VIBRATION_REDUCTION;
    let df = (1.0 - dr * dr).sqrt();
    let t_d = 1.0 / (freq * df);
    let a1 =
        (0.24968 + 0.24961 * v) + ((0.80008 + 1.23328 * v) + (0.49599 + 3.17316 * v) * dr) * dr;
    let a3 =
        (0.25149 + 0.21474 * v) + ((-0.83249 + 1.41498 * v) + (0.85181 - 4.90094 * v) * dr) * dr;
    let a2 = 1.0 - a1 - a3;
    let t2 = 0.4999
        + (((0.46159 + 8.57843 * v) * v)
            + (((4.26169 - 108.644 * v) * v) + ((1.75601 + 336.989 * v) * v) * dr) * dr)
            * dr;
    Ok((vec![a1, a2, a3], vec![0.0, t2 * t_d, t_d]))
}

fn expansion(freq: f64, dr: f64, t: &[[f64; 4]], a: &[[f64; 4]]) -> Built {
    let tau = 1.0 / freq;
    let k = 4;
    let (mut ts, mut amps) = (Vec::new(), Vec::new());
    for i in 0..t.len() {
        let mut u = t[i][k - 1];
        let mut v = a[i][k - 1];
        for j in 0..k - 1 {
            u = u * dr + t[i][k - j - 2];
            v = v * dr + a[i][k - j - 2];
        }
        ts.push(u * tau);
        amps.push(v);
    }
    Ok((amps, ts))
}

fn hump2(freq: f64, dr: f64) -> Built {
    let t = [
        [0.0, 0.0, 0.0, 0.0],
        [0.49890, 0.16270, -0.54262, 6.16180],
        [0.99748, 0.18382, -1.58270, 8.17120],
        [1.49920, -0.09297, -0.28338, 1.85710],
    ];
    let a = [
        [0.16054, 0.76699, 2.26560, -1.22750],
        [0.33911, 0.45081, -2.58080, 1.73650],
        [0.34089, -0.61533, -0.68765, 0.42261],
        [0.15997, -0.60246, 1.00280, -0.93145],
    ];
    expansion(freq, dr, &t, &a)
}

fn hump3(freq: f64, dr: f64) -> Built {
    let t = [
        [0.0, 0.0, 0.0, 0.0],
        [0.49974, 0.23834, 0.44559, 12.4720],
        [0.99849, 0.29808, -2.36460, 23.3990],
        [1.49870, 0.10306, -2.01390, 17.0320],
        [1.99960, -0.28231, 0.61536, 5.40450],
    ];
    let a = [
        [0.11275, 0.76632, 3.29160, -1.44380],
        [0.23698, 0.61164, -2.57850, 4.85220],
        [0.30008, -0.19062, -2.14560, 0.13744],
        [0.23775, -0.73297, 0.46885, -2.08650],
        [0.11244, -0.45439, 0.96382, -1.46000],
    ];
    expansion(freq, dr, &t, &a)
}

static ZV: Def = Def {
    name: "zv",
    min_freq: 21.0,
    max_damping: 0.99,
    build: zv,
};
static MZV: Def = Def {
    name: "mzv",
    min_freq: 23.0,
    max_damping: 0.99,
    build: mzv,
};
static ZVD: Def = Def {
    name: "zvd",
    min_freq: 29.0,
    max_damping: 0.99,
    build: zvd,
};
static EI: Def = Def {
    name: "ei",
    min_freq: 29.0,
    max_damping: 0.4,
    build: ei,
};
static HUMP2: Def = Def {
    name: "2hump_ei",
    min_freq: 39.0,
    max_damping: 0.3,
    build: hump2,
};
static HUMP3: Def = Def {
    name: "3hump_ei",
    min_freq: 48.0,
    max_damping: 0.2,
    build: hump3,
};

/// Looks up a built-in shaper by its exact lowercase name. Names carrying parenthesised
/// arguments are not supported and return `None`.
pub fn builtin(name: &str) -> Option<&'static dyn Shaper> {
    let def: &'static Def = match name {
        "zv" => &ZV,
        "mzv" => &MZV,
        "zvd" => &ZVD,
        "ei" => &EI,
        "2hump_ei" => &HUMP2,
        "3hump_ei" => &HUMP3,
        _ => return None,
    };
    Some(def)
}
