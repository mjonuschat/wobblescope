//! Per-chip, worst-case and summed analysis across several chips and probe points.

use serde::{Deserialize, Serialize};

use crate::CORE_VERSION;
use crate::eligibility::{DatasetInput, Eligibility, Health, select_eligible};
use crate::error::CoreError;
use crate::fit::{FREQ_STEP, FitParams, Recommendation, find_best_shaper};
use crate::numeric::{arange, interp};
use crate::psd::{CalibrationData, MAX_FREQ};

/// Display-only sum of several datasets' total PSD on a shared grid. No recommendation is
/// ever computed from it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// An input that could not be turned into a PSD and was not eligible anyway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Recommendation from the chip's eligible datasets; `None` when it has none.
    pub recommendation: Option<Recommendation>,
}

/// Full multi-chip analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    /// One entry per chip, in order of first appearance.
    pub per_chip: Vec<ChipAnalysis>,
    /// Recommendation over every eligible dataset of every chip; `None` when none is eligible.
    pub worst_case: Option<Recommendation>,
    /// Summed PSD of the eligible datasets, for display only.
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
/// recommendation over all eligible datasets, and the display-only summed PSD.
///
/// Inputs with a `window` are cut to it first. An eligible input that cannot be
/// processed is an error; an ineligible one is listed in `failures` and does not block the rest.
///
/// # Errors
/// Any [`CoreError`] from building a PSD or from fitting.
pub fn analyze(inputs: &[DatasetInput], params: &FitParams) -> Result<Analysis, CoreError> {
    let (eligible, eligibility) = select_eligible(inputs);

    let mut psds: Vec<Option<CalibrationData>> = Vec::with_capacity(inputs.len());
    let mut windows: Vec<Option<std::ops::Range<usize>>> = vec![None; inputs.len()];
    let mut failures = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        let name = format!("{}#{}", input.chip, input.point);
        let built = (|| {
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
        })();
        match built {
            Ok(cd) => psds.push(Some(cd)),
            Err(e) if eligible.contains(&i) => return Err(e),
            Err(e) => {
                failures.push(DatasetFailure {
                    chip: input.chip.clone(),
                    point: input.point,
                    error: e.to_string(),
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
            .filter(|i| eligible.contains(i))
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

    let chosen: Vec<CalibrationData> = eligible.iter().filter_map(|&i| psds[i].clone()).collect();
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
