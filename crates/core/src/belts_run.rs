//! Belt comparison of one chip's two recordings, one per belt direction.

use serde::{Deserialize, Serialize};

use crate::CORE_VERSION;
use crate::belts::{BeltsResult, compare_psds};
use crate::eligibility::{DatasetDecision, DatasetInput, select_eligible};
use crate::error::CoreError;
use crate::multichip::windowed_samples;
use crate::psd::CalibrationData;

/// The summed PSD of one recording, on the recording's own bins up to the analysis `max_freq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BeltSpectrum {
    /// Frequency bins in Hz.
    pub freqs: Vec<f64>,
    /// Summed, not normalised, PSD.
    pub psd: Vec<f64>,
}

/// What became of one belt's recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BeltSide {
    /// Whether the session has a recording for this belt and chip.
    pub present: bool,
    /// Why the recording was or was not used; `None` when it is absent.
    pub decision: Option<DatasetDecision>,
    /// Why an eligible recording's PSD could not be built.
    pub failure: Option<String>,
    /// The recording's PSD; kept whenever the recording is eligible and its PSD was built, even
    /// when the other belt has none.
    pub spectrum: Option<BeltSpectrum>,
}

/// Comparison of the two belts of one chip at one probe point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BeltsAnalysis {
    /// Chip name.
    pub chip: String,
    /// Probe point index.
    pub point: usize,
    pub belt_a: BeltSide,
    pub belt_b: BeltSide,
    /// The comparison; `None` unless both belts have a usable spectrum.
    pub result: Option<BeltsResult>,
    /// `wobblescope-core` version that produced it.
    pub core_version: String,
}

fn spectrum_of(input: &DatasetInput, max_freq: f64) -> Result<BeltSpectrum, String> {
    if let Some(message) = &input.load_error {
        return Err(message.clone());
    }
    let build = || -> Result<CalibrationData, CoreError> {
        let (samples, _) = windowed_samples(input)?;
        CalibrationData::from_samples(&format!("{}#{}", input.chip, input.point), &samples)
    };
    let cd = build().map_err(|e| e.to_string())?;
    let keep = cd.freq_bins.iter().take_while(|f| **f <= max_freq).count();
    Ok(BeltSpectrum {
        freqs: cd.freq_bins[..keep].to_vec(),
        psd: cd.psd_sum[..keep].to_vec(),
    })
}

fn side(input: Option<&DatasetInput>, max_freq: f64) -> BeltSide {
    let Some(input) = input else {
        return BeltSide {
            present: false,
            decision: None,
            failure: None,
            spectrum: None,
        };
    };
    let (eligible, mut eligibility) = select_eligible(std::slice::from_ref(input));
    let mut decision = eligibility.decisions.remove(0);
    let (failure, spectrum) = if eligible.is_empty() {
        (None, None)
    } else {
        match spectrum_of(input, max_freq) {
            Ok(spectrum) => (None, Some(spectrum)),
            Err(error) => {
                decision.dropped = Some(error.clone());
                (Some(error), None)
            }
        }
    };
    BeltSide {
        present: true,
        decision: Some(decision),
        failure,
        spectrum,
    }
}

/// Compares the belt A and belt B recordings of `chip` at `point` up to `max_freq`.
///
/// Each recording is cut to its time window and judged by the same eligibility rules as an axis
/// recording. Its summed PSD, not normalised, goes to [`compare_psds`] only when both belts are
/// eligible and their PSDs could be built; otherwise `result` is `None` and each usable side
/// still carries its own spectrum.
pub fn analyze_belts(
    chip: &str,
    point: usize,
    a: Option<&DatasetInput>,
    b: Option<&DatasetInput>,
    max_freq: f64,
) -> BeltsAnalysis {
    let belt_a = side(a, max_freq);
    let belt_b = side(b, max_freq);
    let result = match (&belt_a.spectrum, &belt_b.spectrum) {
        (Some(sa), Some(sb)) => Some(compare_psds(
            &sa.freqs, &sa.psd, &sb.freqs, &sb.psd, max_freq,
        )),
        _ => None,
    };
    BeltsAnalysis {
        chip: chip.to_owned(),
        point,
        belt_a,
        belt_b,
        result,
        core_version: CORE_VERSION.to_owned(),
    }
}
