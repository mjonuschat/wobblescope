//! Shaper fitting and recommendation, ported from Klipper's `fit_shaper` and
//! `find_best_shaper` (parity mode: damping fixed, frequency searched in 0.2 Hz steps).

use serde::{Deserialize, Serialize};

use crate::CORE_VERSION;
use crate::error::CoreError;
use crate::numeric::{arange, interp};
use crate::psd::{CalibrationData, MAX_FREQ};
use crate::score::{find_max_accel, remaining_vibrations, shaper_smoothing};
use crate::shaper::{AUTOTUNE_SHAPERS, DEFAULT_DAMPING_RATIO, Shaper, builtin};

/// Damping ratios the robustness check is evaluated across.
pub const TEST_DAMPING_RATIOS: [f64; 3] = [0.075, 0.1, 0.15];
/// Upper bound of the searched shaper frequency in Hz.
pub const MAX_SHAPER_FREQ: f64 = 150.0;
/// Step of the searched shaper frequency in Hz.
pub const FREQ_STEP: f64 = 0.2;
const SMOOTHING_ACCEL: f64 = 5000.0;

/// Caller-supplied fitting parameters. `None` selects Klipper's default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct FitParams {
    /// Square corner velocity in mm/s used for smoothing.
    pub scv: f64,
    /// Damping ratio the shaper is built with (default 0.1).
    pub damping_ratio: Option<f64>,
    /// Stop searching higher frequencies once smoothing exceeds this (mm).
    pub max_smoothing: Option<f64>,
    /// Remaining-vibration level (fraction) above which less smoothing is not preferred.
    pub max_vibrations: Option<f64>,
    /// Damping ratios for the robustness check (default [`TEST_DAMPING_RATIOS`]).
    pub test_damping_ratios: Option<Vec<f64>>,
    /// Upper PSD frequency considered (default 200 Hz).
    pub max_freq: Option<f64>,
    /// Shapers to evaluate, in order (default [`AUTOTUNE_SHAPERS`]).
    pub shapers: Option<Vec<String>>,
}

impl Default for FitParams {
    fn default() -> Self {
        Self {
            scv: 5.0,
            damping_ratio: None,
            max_smoothing: None,
            max_vibrations: None,
            test_damping_ratios: None,
            max_freq: None,
            shapers: None,
        }
    }
}

/// The effective values a fit used, saved with every result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FitSettings {
    /// Square corner velocity in mm/s.
    pub scv: f64,
    /// Damping ratio the shapers were built with.
    pub damping_ratio: f64,
    /// Damping ratios of the robustness check.
    pub test_damping_ratios: Vec<f64>,
    /// Smoothing limit, if any.
    pub max_smoothing: Option<f64>,
    /// Vibration preference level, if any.
    pub max_vibrations: Option<f64>,
    /// Largest PSD frequency considered, in Hz.
    pub max_freq: f64,
    /// Frequency search step in Hz.
    pub freq_step: f64,
    /// Shapers evaluated, in order.
    pub shapers: Vec<String>,
}

/// One fitted shaper at one frequency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CalibrationResult {
    /// Shaper name.
    pub name: String,
    /// Shaper frequency in Hz.
    pub freq: f64,
    /// Frequency grid of `vals`.
    pub freq_bins: Vec<f64>,
    /// Worst-case shaper response over all datasets and test damping ratios.
    pub vals: Vec<f64>,
    /// Remaining vibrations as a fraction.
    pub vibrs: f64,
    /// Estimated corner smoothing in mm.
    pub smoothing: f64,
    /// Combined score; lower is better.
    pub score: f64,
    /// Largest acceleration keeping smoothing within the target. A smoothing-based
    /// recommendation, not a machine limit.
    pub max_accel: f64,
}

/// The outcome of a shaper search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Recommendation {
    /// The recommended shaper.
    pub best: CalibrationResult,
    /// The fitted result of every evaluated shaper, in evaluation order.
    pub all: Vec<CalibrationResult>,
    /// The settings used.
    pub settings: FitSettings,
    /// `wobblescope-core` version that produced it.
    pub core_version: String,
}

/// Axis of an exported configuration snippet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Axis {
    /// X axis.
    X,
    /// Y axis.
    Y,
}

impl Recommendation {
    /// Klipper `[input_shaper]` lines for `axis`. The fitted damping ratio is exported too, so
    /// applying the snippet reproduces the impulses that were fitted.
    pub fn config_snippet(&self, axis: Axis) -> String {
        let a = match axis {
            Axis::X => "x",
            Axis::Y => "y",
        };
        format!(
            "[input_shaper]\nshaper_type_{a}: {}\nshaper_freq_{a}: {:.1}\ndamping_ratio_{a}: {}\n",
            self.best.name, self.best.freq, self.settings.damping_ratio
        )
    }
}

/// Most memory one shaper fit may retain for its candidates' `freq_bins` and `vals` arrays
/// (every candidate keeps two arrays on the 0.2 Hz grid). Keeps a large `max_freq` from
/// exhausting memory, notably in 32-bit wasm32.
const MAX_RETAINED_BYTES: usize = 256 << 20;

/// Largest accepted `max_freq`; bounds the size of the evaluation grid.
const MAX_FREQ_LIMIT: f64 = 100_000.0;

fn invalid(what: &str, v: impl std::fmt::Debug) -> CoreError {
    CoreError::InvalidParameter(format!("{what} = {v:?}"))
}

pub(crate) fn validate_params(p: &FitParams) -> Result<(), CoreError> {
    let non_negative = |v: f64| v.is_finite() && v >= 0.0;
    let ratio = |v: f64| v.is_finite() && (0.0..1.0).contains(&v);
    if !non_negative(p.scv) {
        return Err(invalid("scv", p.scv));
    }
    if let Some(d) = p.damping_ratio.filter(|d| !ratio(*d)) {
        return Err(invalid("damping_ratio", d));
    }
    if let Some(bad) = p.test_damping_ratios.iter().flatten().find(|d| !ratio(**d)) {
        return Err(invalid("test_damping_ratios", bad));
    }
    for (what, v) in [
        ("max_smoothing", p.max_smoothing),
        ("max_vibrations", p.max_vibrations),
    ] {
        if let Some(v) = v.filter(|v| !non_negative(*v)) {
            return Err(invalid(what, v));
        }
    }
    if let Some(f) = p
        .max_freq
        .filter(|f| !(non_negative(*f) && *f <= MAX_FREQ_LIMIT))
    {
        return Err(invalid("max_freq", f));
    }
    Ok(())
}

fn validate_dataset(d: &CalibrationData) -> Result<(), CoreError> {
    let bad = |why: &str| CoreError::InvalidParameter(format!("dataset {:?}: {why}", d.name));
    if d.freq_bins.is_empty() {
        return Err(bad("no frequency bins"));
    }
    if d.psd_sum.len() != d.freq_bins.len() {
        return Err(bad("psd_sum and freq_bins differ in length"));
    }
    if d.freq_bins.iter().chain(&d.psd_sum).any(|v| !v.is_finite()) {
        return Err(bad("non-finite values"));
    }
    if d.freq_bins.windows(2).any(|w| w[1] <= w[0]) {
        return Err(bad("frequency bins are not increasing"));
    }
    Ok(())
}

fn or_default<T>(v: Option<T>, is_unset: impl Fn(&T) -> bool, default: T) -> T {
    match v {
        Some(v) if !is_unset(&v) => v,
        _ => default,
    }
}

fn damping_of(params: &FitParams) -> f64 {
    or_default(params.damping_ratio, |d| *d == 0.0, DEFAULT_DAMPING_RATIO)
}

fn test_ratios_of(params: &FitParams) -> Vec<f64> {
    or_default(
        params.test_damping_ratios.clone(),
        Vec::is_empty,
        TEST_DAMPING_RATIOS.to_vec(),
    )
}

fn test_freqs_of(shaper: &dyn Shaper) -> Vec<f64> {
    let freq_end = MAX_SHAPER_FREQ;
    let freq_start = shaper.min_freq().min(freq_end - 1e-7);
    arange(freq_start, freq_end, FREQ_STEP)
}

fn effective_max_freq(params: &FitParams, test_freqs: &[f64]) -> f64 {
    let top = test_freqs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    or_default(params.max_freq, |f| *f == 0.0, MAX_FREQ).max(top)
}

fn shaper_names(params: &FitParams) -> Vec<String> {
    match &params.shapers {
        Some(s) if !s.is_empty() => s.clone(),
        _ => AUTOTUNE_SHAPERS.iter().map(|s| (*s).to_owned()).collect(),
    }
}

/// Checks that every shaper `params` selects exists and can be built with the damping ratio the
/// fit uses, so settings the fit would refuse fail even when no dataset reaches the fit.
/// `params` are assumed to have passed [`validate_params`].
///
/// # Errors
/// [`CoreError::UnknownShaper`] or [`CoreError::ShaperInit`].
pub(crate) fn validate_shapers(params: &FitParams) -> Result<(), CoreError> {
    let damping_ratio = damping_of(params);
    for name in shaper_names(params) {
        let shaper = builtin(&name).ok_or(CoreError::UnknownShaper(name))?;
        shaper.impulses(shaper.min_freq(), damping_ratio)?;
    }
    Ok(())
}

/// The lowest `max_freq` any shaper evaluated under `params` reads the PSD up to; the input of
/// [`check_dataset`]. `params` are assumed valid.
pub(crate) fn lowest_fit_max_freq(params: &FitParams) -> f64 {
    shaper_names(params)
        .iter()
        .filter_map(|name| builtin(name))
        .map(|shaper| effective_max_freq(params, &test_freqs_of(shaper)))
        .fold(f64::INFINITY, f64::min)
}

/// Checks that `d` can be one of [`fit_shaper`]'s datasets: well formed, no negative values, and
/// positive power at or below `max_freq`. The fit runs it on every dataset; `analyze` runs it with
/// [`lowest_fit_max_freq`] to drop a recording before it can fail a fit.
///
/// # Errors
/// [`CoreError::InvalidParameter`] for a malformed dataset or a negative PSD value, and
/// [`CoreError::FlatSignal`] when no bin the fit reads has power.
pub(crate) fn check_dataset(d: &CalibrationData, max_freq: f64) -> Result<(), CoreError> {
    validate_dataset(d)?;
    if d.psd_sum.iter().any(|p| *p < 0.0) {
        return Err(CoreError::InvalidParameter(format!(
            "dataset {:?}: negative PSD values",
            d.name
        )));
    }
    let powered = d
        .freq_bins
        .iter()
        .zip(&d.psd_sum)
        .any(|(f, p)| *f <= max_freq && *p > 0.0);
    if powered {
        Ok(())
    } else {
        Err(CoreError::FlatSignal)
    }
}

/// Fits one shaper over `data` (expected to be normalised, one element per dataset).
///
/// Returns the selected result first, then every candidate in descending-frequency order.
///
/// # Errors
/// [`CoreError::InvalidParameter`] for malformed parameters, [`CoreError::FlatSignal`] for a
/// PSD without power, [`CoreError::ShaperInit`] if the shaper
/// cannot be built, and [`CoreError::TooFewSamples`] for an empty dataset list.
pub fn fit_shaper(
    shaper: &dyn Shaper,
    data: &[CalibrationData],
    params: &FitParams,
) -> Result<Vec<CalibrationResult>, CoreError> {
    validate_params(params)?;
    if data.is_empty() {
        return Err(CoreError::TooFewSamples { have: 0, need: 1 });
    }
    let damping_ratio = damping_of(params);
    let test_ratios = test_ratios_of(params);
    let test_freqs = test_freqs_of(shaper);
    let max_freq = effective_max_freq(params, &test_freqs);
    for d in data {
        check_dataset(d, max_freq)?;
    }

    let mut min_freq = max_freq;
    for d in data {
        min_freq = min_freq.min(d.freq_bins.iter().copied().fold(f64::INFINITY, f64::min));
    }

    let grid_len = ((max_freq - min_freq) / FREQ_STEP).ceil().max(0.0);
    // One more result than candidates: the selected one is returned first, as a clone.
    let retained =
        (test_freqs.len() + 1) as f64 * grid_len * 2.0 * std::mem::size_of::<f64>() as f64;
    if retained > MAX_RETAINED_BYTES as f64 {
        return Err(CoreError::InvalidParameter(format!(
            "max_freq = {max_freq} would retain about {:.0} MiB of candidate results \
             (limit {} MiB)",
            retained / (1 << 20) as f64,
            MAX_RETAINED_BYTES >> 20
        )));
    }

    let mut best: Option<usize> = None;
    let mut results: Vec<CalibrationResult> = Vec::new();
    for &test_freq in test_freqs.iter().rev() {
        let imp = shaper.impulses(test_freq, damping_ratio)?;
        let smoothing = shaper_smoothing(&imp, SMOOTHING_ACCEL, params.scv);
        if let Some(limit) = params.max_smoothing
            && limit != 0.0
            && smoothing > limit
            && let Some(b) = best
        {
            let mut out = vec![results[b].clone()];
            out.extend(results);
            return Ok(out);
        }
        let max_accel = find_max_accel(&imp, params.scv);

        let mut vibrations_max = 0.0_f64;
        let mut per_dataset: Vec<(Vec<f64>, Vec<f64>)> = Vec::new();
        for d in data {
            let keep = d.freq_bins.iter().take_while(|f| **f <= max_freq).count();
            let bins = &d.freq_bins[..keep];
            let psd = &d.psd_sum[..keep];
            let mut vals_max = vec![0.0_f64; bins.len()];
            for &dr in &test_ratios {
                let (vibrations, vals) = remaining_vibrations(&imp, dr, bins, psd)?;
                for (m, v) in vals_max.iter_mut().zip(&vals) {
                    *m = m.max(*v);
                }
                if vibrations > vibrations_max {
                    vibrations_max = vibrations;
                }
            }
            per_dataset.push((bins.to_vec(), vals_max));
        }
        let score = smoothing * (vibrations_max.powf(1.5) + vibrations_max * 0.2 + 0.01);
        let grid = arange(min_freq, max_freq, FREQ_STEP);
        let mut vals = vec![0.0_f64; grid.len()];
        for (bins, v) in &per_dataset {
            for (m, x) in vals.iter_mut().zip(interp(&grid, bins, v)) {
                *m = m.max(x);
            }
        }
        results.push(CalibrationResult {
            name: shaper.name().to_owned(),
            freq: test_freq,
            freq_bins: grid,
            vals,
            vibrs: vibrations_max,
            smoothing,
            score,
            max_accel,
        });
        let last = results.len() - 1;
        if best.is_none_or(|b| results[b].vibrs > results[last].vibrs) {
            best = Some(last);
        }
    }

    let Some(best_idx) = best else {
        return Err(CoreError::TooFewSamples { have: 0, need: 1 });
    };
    let best_vibrs = results[best_idx].vibrs;
    let mut selected = best_idx;
    for i in (0..results.len()).rev() {
        let res = &results[i];
        if res.vibrs < best_vibrs * 1.1 + 0.0005
            && (res.score < results[selected].score
                || params.max_vibrations.is_some_and(|mv| best_vibrs > mv))
        {
            selected = i;
        }
    }
    let mut out = vec![results[selected].clone()];
    out.extend(results);
    Ok(out)
}

/// Fits every requested shaper and picks the recommended one, as Klipper's `find_best_shaper`.
///
/// `data` holds one element per dataset (recording or probe point); the result is the
/// worst case across all of them.
///
/// # Errors
/// [`CoreError::UnknownShaper`] for a name that is not a built-in, [`CoreError::ShaperInit`]
/// for a shaper that cannot be built with the damping ratio, plus the errors of [`fit_shaper`].
pub fn find_best_shaper(
    data: &[CalibrationData],
    params: &FitParams,
) -> Result<Recommendation, CoreError> {
    validate_params(params)?;
    validate_shapers(params)?;
    let names = shaper_names(params);
    let mut best: Option<CalibrationResult> = None;
    let mut all: Vec<CalibrationResult> = Vec::new();
    let mut max_freq_used = f64::NEG_INFINITY;

    for name in &names {
        let shaper = builtin(name).ok_or_else(|| CoreError::UnknownShaper(name.clone()))?;
        max_freq_used = max_freq_used.max(effective_max_freq(params, &test_freqs_of(shaper)));

        let mut fit = fit_shaper(shaper, data, params)?;
        let mut shaper_res = fit.remove(0);
        let results = fit;

        let take = match &best {
            None => true,
            Some(b) => {
                shaper_res.score * 1.2 < b.score
                    || (shaper_res.score * 1.05 < b.score
                        && shaper_res.smoothing * 1.1 < b.smoothing)
                    || params
                        .max_vibrations
                        .is_some_and(|mv| b.vibrs > mv && shaper_res.vibrs < b.vibrs)
            }
        };
        if take {
            best = Some(shaper_res.clone());
        }
        for s in results.iter().rev() {
            let b = best.as_ref().expect("best is set above");
            if s.vibrs < b.vibrs && s.smoothing < b.smoothing {
                best = Some(s.clone());
                shaper_res = s.clone();
            }
        }
        all.push(shaper_res);
    }

    let mut best = best.expect("at least one shaper is evaluated");
    if best.name == "zv"
        && let Some(t) = all
            .iter()
            .find(|t| t.name != "zv" && t.vibrs * 1.1 < best.vibrs)
    {
        best = t.clone();
    }

    Ok(Recommendation {
        best,
        all,
        settings: FitSettings {
            scv: params.scv,
            damping_ratio: damping_of(params),
            test_damping_ratios: test_ratios_of(params),
            max_smoothing: params.max_smoothing,
            max_vibrations: params.max_vibrations,
            max_freq: max_freq_used,
            freq_step: FREQ_STEP,
            shapers: names,
        },
        core_version: CORE_VERSION.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A normalised PSD on a 1 Hz grid to 400 Hz with power only above `from` Hz.
    fn powered_above(from: f64) -> CalibrationData {
        let freq_bins: Vec<f64> = (0..=400).map(f64::from).collect();
        let psd_sum: Vec<f64> = freq_bins
            .iter()
            .map(|f| if *f > from { 1.0 } else { 0.0 })
            .collect();
        CalibrationData {
            name: "high".to_owned(),
            psd_x: psd_sum.clone(),
            psd_y: vec![0.0; freq_bins.len()],
            psd_z: vec![0.0; freq_bins.len()],
            psd_sum,
            freq_bins,
            normalized: true,
        }
    }

    #[test]
    fn power_only_above_the_fit_range_is_a_flat_signal_for_the_fit() {
        let params = FitParams::default();
        let max_freq = lowest_fit_max_freq(&params);
        let d = powered_above(max_freq + 10.0);
        assert_eq!(check_dataset(&d, max_freq), Err(CoreError::FlatSignal));
        assert_eq!(check_dataset(&d, f64::INFINITY), Ok(()));
        // The fit refuses it the same way, so analyze must drop it before fitting.
        let zv = builtin("zv").expect("built-in");
        assert_eq!(fit_shaper(zv, &[d], &params), Err(CoreError::FlatSignal));
    }

    #[test]
    fn power_inside_the_fit_range_passes_the_check() {
        let max_freq = lowest_fit_max_freq(&FitParams::default());
        assert_eq!(check_dataset(&powered_above(50.0), max_freq), Ok(()));
    }
}
