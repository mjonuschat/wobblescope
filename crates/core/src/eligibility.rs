//! Which datasets take part in recommendations (spec "Capture health").

use serde::{Deserialize, Serialize};

use crate::samples::RawSamples;

/// Capture health of a chip for a test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Health {
    /// No problems detected.
    Ok,
    /// Problems detected; the data is not trusted.
    Degraded,
    /// The chip type cannot report health; analysed with a warning.
    Unknown,
}

/// Whether the capture covers the whole segment window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Coverage {
    /// All expected samples are present.
    Complete,
    /// Samples are missing.
    Incomplete {
        /// The caller's description of what is absent, echoed in results.
        missing: String,
    },
}

/// One (chip, probe point) recording for one test axis.
#[derive(Debug, Clone)]
pub struct DatasetInput {
    /// Chip name.
    pub chip: String,
    /// Probe point index.
    pub point: usize,
    /// The recording.
    pub samples: RawSamples,
    /// Capture health of the chip.
    pub health: Health,
    /// Coverage of the segment.
    pub coverage: Coverage,
    /// Include the dataset even if health or coverage would exclude it.
    pub include_override: bool,
    /// Keep only samples whose timestamps fall in this range (seconds, the clock of the sample
    /// timestamps, which is Klipper's `print_time`). `None` keeps
    /// every sample.
    pub window: Option<std::ops::Range<f64>>,
}

/// How a dataset was treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
    /// Used.
    Included,
    /// Not used.
    Excluded,
    /// Used only because of the override.
    Overridden,
}

/// The decision for one dataset, with the reasons behind it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetDecision {
    /// Chip name.
    pub chip: String,
    /// Probe point index.
    pub point: usize,
    /// The decision.
    pub decision: Decision,
    /// Human-readable reasons (warnings for included datasets).
    pub reasons: Vec<String>,
}

/// Decisions for all inputs, in input order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eligibility {
    /// One decision per input.
    pub decisions: Vec<DatasetDecision>,
}

/// Decides which inputs are eligible: health not `Degraded` and coverage complete, or the
/// override. Returns the eligible input indices and the decision log.
pub fn select_eligible(inputs: &[DatasetInput]) -> (Vec<usize>, Eligibility) {
    let mut eligible = Vec::new();
    let mut decisions = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        let mut blockers = Vec::new();
        if input.health == Health::Degraded {
            blockers.push("degraded capture health".to_owned());
        }
        if let Coverage::Incomplete { missing } = &input.coverage {
            blockers.push(format!("incomplete coverage: {missing}"));
        }
        let mut reasons = blockers.clone();
        if input.health == Health::Unknown {
            reasons.push("unknown capture health".to_owned());
        }
        let decision = match (blockers.is_empty(), input.include_override) {
            (true, _) => Decision::Included,
            (false, true) => Decision::Overridden,
            (false, false) => Decision::Excluded,
        };
        if decision != Decision::Excluded {
            eligible.push(i);
        }
        decisions.push(DatasetDecision {
            chip: input.chip.clone(),
            point: input.point,
            decision,
            reasons,
        });
    }
    (eligible, Eligibility { decisions })
}
