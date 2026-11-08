//! Per-chip, worst-case and summed analysis across several chips and probe points.

use serde::{Deserialize, Serialize};

use crate::CORE_VERSION;
use crate::eligibility::{DatasetInput, Eligibility, Health, select_eligible};
use crate::error::CoreError;
use crate::fit::{
    FREQ_STEP, FitParams, Recommendation, check_dataset, find_best_shaper, lowest_fit_max_freq,
    validate_params,
};
use crate::numeric::{arange, interp};
use crate::psd::{CalibrationData, MAX_FREQ};

/// Display-only sum of several datasets' total PSD on a shared grid. No recommendation is
/// ever computed from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SummedPsd {
    /// Frequency grid in Hz.
    pub freq_bins: Vec<f64>,
    /// Summed PSD.
    pub psd: Vec<f64>,
}

/// Sums every dataset's `psd_sum`, interpolated onto `arange(min_bin, max_freq, 0.2)` where
/// `min_bin` is the smallest first bin. A dataset contributes only within its own measured
/// range, so a low-rate chip adds nothing above its Nyquist frequency. `None` for empty input.
pub fn summed_psd(data: &[CalibrationData], max_freq: f64) -> Option<SummedPsd> {
    let min_bin = data
        .iter()
        .filter_map(|d| d.freq_bins.first().copied())
        .reduce(f64::min)?;
    let grid = arange(min_bin, max_freq, FREQ_STEP);
    let mut psd = vec![0.0; grid.len()];
    for d in data.iter().filter(|d| !d.freq_bins.is_empty()) {
        let (first, last) = (d.freq_bins[0], d.freq_bins[d.freq_bins.len() - 1]);
        for ((acc, &f), v) in psd
            .iter_mut()
            .zip(&grid)
            .zip(interp(&grid, &d.freq_bins, &d.psd_sum))
        {
            if (first..=last).contains(&f) {
                *acc += v;
            }
        }
    }
    Some(SummedPsd {
        freq_bins: grid,
        psd,
    })
}

/// An input that could not be processed and was not eligible anyway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DatasetFailure {
    /// Chip name.
    pub chip: String,
    /// Probe point index.
    pub point: usize,
    /// Why the input could not be processed.
    pub error: String,
}

/// Analysis of one chip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChipAnalysis {
    /// Chip name.
    pub chip: String,
    /// Worst health among the chip's inputs (`Degraded` > `Unknown` > `Ok`).
    pub health: Health,
    /// Normalised PSD of every input, keyed by probe point, eligible or not.
    pub datasets: Vec<(usize, CalibrationData)>,
    /// For inputs analysed with a time `window`, the sample index range kept, keyed by probe
    /// point.
    pub windows: Vec<(usize, std::ops::Range<usize>)>,
    /// Recommendation from the chip's eligible datasets that could be analysed; `None` when it
    /// has none.
    pub recommendation: Option<Recommendation>,
}

/// Full multi-chip analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Analysis {
    /// One entry per chip, in order of first appearance.
    pub per_chip: Vec<ChipAnalysis>,
    /// Recommendation over every eligible dataset of every chip that could be analysed; `None`
    /// when there is none.
    pub worst_case: Option<Recommendation>,
    /// Summed PSD of the same datasets as `worst_case`, for display only.
    pub summed: Option<SummedPsd>,
    /// Ineligible inputs that could not be processed; shown, never analysed around.
    pub failures: Vec<DatasetFailure>,
    /// Why each input was or was not used.
    pub eligibility: Eligibility,
    /// `wobblescope-core` version that produced it.
    pub core_version: String,
}

fn worse(a: Health, b: Health) -> Health {
    let rank = |h| match h {
        Health::Ok => 0,
        Health::Unknown => 1,
        Health::Degraded => 2,
    };
    if rank(b) > rank(a) { b } else { a }
}

/// Analyses all inputs: a normalised PSD for each, a recommendation per chip, a worst-case
/// recommendation over all usable eligible datasets, and the display-only summed PSD.
///
/// Inputs with a `window` are cut to it first. An eligible input that cannot be analysed (its
/// samples did not load, its PSD cannot be built, or the fit would reject it) is left out of
/// every fit and the summed PSD, and its decision says why in `dropped`. An ineligible input
/// that cannot be processed is listed in `failures`. Neither blocks the rest.
///
/// # Errors
/// [`CoreError::InvalidParameter`] for invalid fit parameters, and any other [`CoreError`] a
/// fit returns for the usable datasets.
pub fn analyze(inputs: &[DatasetInput], params: &FitParams) -> Result<Analysis, CoreError> {
    validate_params(params)?;
    let (eligible, mut eligibility) = select_eligible(inputs);
    let fit_max_freq = lowest_fit_max_freq(params);

    let mut psds: Vec<Option<CalibrationData>> = Vec::with_capacity(inputs.len());
    let mut windows: Vec<Option<std::ops::Range<usize>>> = vec![None; inputs.len()];
    let mut usable: Vec<usize> = Vec::with_capacity(eligible.len());
    let mut failures = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        let name = format!("{}#{}", input.chip, input.point);
        let built = match &input.load_error {
            Some(message) => Err(message.clone()),
            None => (|| {
                let mut cd = if let Some(w) = &input.window {
                    let kept = input.samples.window(w.clone())?;
                    let samples = input.samples.slice(kept.clone());
                    windows[i] = Some(kept);
                    CalibrationData::from_samples(&name, &samples)?
                } else {
                    CalibrationData::from_samples(&name, &input.samples)?
                };
                cd.normalize_to_frequencies();
                Ok::<_, CoreError>(cd)
            })()
            .map_err(|e| e.to_string()),
        };
        let is_eligible = eligible.contains(&i);
        match built {
            Ok(cd) => {
                if is_eligible {
                    match check_dataset(&cd, fit_max_freq) {
                        Ok(()) => usable.push(i),
                        Err(e) => eligibility.decisions[i].dropped = Some(e.to_string()),
                    }
                }
                psds.push(Some(cd));
            }
            Err(error) if is_eligible => {
                eligibility.decisions[i].dropped = Some(error);
                psds.push(None);
            }
            Err(error) => {
                failures.push(DatasetFailure {
                    chip: input.chip.clone(),
                    point: input.point,
                    error,
                });
                psds.push(None);
            }
        }
    }

    let mut chips: Vec<&str> = Vec::new();
    for input in inputs {
        if !chips.contains(&input.chip.as_str()) {
            chips.push(&input.chip);
        }
    }
    let mut per_chip = Vec::new();
    for chip in chips {
        let idx: Vec<usize> = (0..inputs.len())
            .filter(|&i| inputs[i].chip == chip)
            .collect();
        let health = idx
            .iter()
            .fold(Health::Ok, |h, &i| worse(h, inputs[i].health));
        let chosen: Vec<CalibrationData> = idx
            .iter()
            .filter(|i| usable.contains(i))
            .filter_map(|&i| psds[i].clone())
            .collect();
        let recommendation = if chosen.is_empty() {
            None
        } else {
            Some(find_best_shaper(&chosen, params)?)
        };
        per_chip.push(ChipAnalysis {
            chip: chip.to_owned(),
            health,
            windows: idx
                .iter()
                .filter_map(|&i| windows[i].clone().map(|r| (inputs[i].point, r)))
                .collect(),
            datasets: idx
                .iter()
                .filter_map(|&i| psds[i].clone().map(|p| (inputs[i].point, p)))
                .collect(),
            recommendation,
        });
    }

    let chosen: Vec<CalibrationData> = usable.iter().filter_map(|&i| psds[i].clone()).collect();
    let (worst_case, summed) = if chosen.is_empty() {
        (None, None)
    } else {
        let max_freq = params.max_freq.filter(|f| *f != 0.0).unwrap_or(MAX_FREQ);
        (
            Some(find_best_shaper(&chosen, params)?),
            summed_psd(&chosen, max_freq),
        )
    };

    Ok(Analysis {
        per_chip,
        worst_case,
        summed,
        failures,
        eligibility,
        core_version: CORE_VERSION.to_owned(),
    })
}
