//! Building the session metadata of a capture.

use wobblescope_core::eligibility::Health;
use wobblescope_session::meta::{
    ChipMeta, FORMAT_VERSION, Outcome, PrinterInfo, ProbePoint, SessionMeta, Source, TimeDomain,
};

use super::recorder::ChipSummary;
use super::request::TestPlan;
use crate::moonraker::chips::{Chip, ChipType};

/// What a chip contributes to the metadata before the test runs.
#[derive(Debug, Clone, PartialEq)]
pub struct ChipFacts {
    /// The chip's config section.
    pub section: String,
    /// The chip's type.
    pub chip_type: ChipType,
    /// The rate Klipper's config implies, in Hz.
    pub nominal_rate_hz: Option<f64>,
    /// The rate seen when the chip was proven, in Hz.
    pub observed_rate_hz: f64,
}

impl ChipFacts {
    /// The facts of `chip`, proven at `observed_rate_hz`.
    #[must_use]
    pub fn of(chip: &Chip, observed_rate_hz: f64) -> Self {
        Self {
            section: chip.section.clone(),
            chip_type: chip.chip_type,
            nominal_rate_hz: chip.nominal_rate_hz,
            observed_rate_hz,
        }
    }
}

fn type_name(t: ChipType) -> &'static str {
    match t {
        ChipType::Adxl345 => "adxl345",
        ChipType::Lis2dw => "lis2dw",
        ChipType::Beacon => "beacon",
    }
}

/// The metadata a capture starts with: no segment yet, chips with unknown health. It becomes the
/// first journal.
#[must_use]
pub fn initial(
    plan: &TestPlan,
    id: &str,
    created_at: &str,
    chips: &[ChipFacts],
    printer: Option<PrinterInfo>,
) -> SessionMeta {
    SessionMeta {
        format_version: FORMAT_VERSION,
        id: id.to_owned(),
        created_at: created_at.to_owned(),
        source: Source::Capture,
        time_domain: TimeDomain::KlipperPrintTime,
        mode: Some(plan.mode),
        settings: Some(plan.sweep_settings()),
        printer,
        outcome: Outcome::Complete,
        notes: Vec::new(),
        probe_points: plan
            .points
            .iter()
            .map(|p| ProbePoint { position: Some(*p) })
            .collect(),
        chips: chips
            .iter()
            .map(|c| ChipMeta {
                name: c.section.clone(),
                chip_type: Some(type_name(c.chip_type).to_owned()),
                nominal_rate_hz: c.nominal_rate_hz,
                observed_rate_hz: Some(c.observed_rate_hz),
                axes_map: None,
                mounting: None,
                health: Health::Unknown,
                health_notes: Vec::new(),
            })
            .collect(),
        segments: Vec::new(),
    }
}

/// Writes each chip's health, notes and observed rate into `meta`. `summaries` follow the order
/// of `meta.chips`.
pub fn apply_summaries(meta: &mut SessionMeta, summaries: &[ChipSummary]) {
    for (chip, summary) in meta.chips.iter_mut().zip(summaries) {
        chip.health = summary.health;
        chip.health_notes.clone_from(&summary.notes);
        if let Some(rate) = summary
            .observed_rate_hz
            .filter(|r| r.is_finite() && *r > 0.0)
        {
            chip.observed_rate_hz = Some(rate);
        }
    }
}
