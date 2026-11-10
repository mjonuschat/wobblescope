//! The shapes of a saved result: the record inside the zip and the API's request and reply types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wobblescope_core::fit::{Axis, FitParams};
use wobblescope_core::multichip::Analysis;
use wobblescope_session::meta::SessionMeta;

/// Version of [`ResultRecord`]; a reader refuses a newer one.
pub const RESULT_VERSION: u32 = 1;

/// The configuration snippets the web app rendered for one axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Snippets {
    /// Snippet for the worst-case recommendation, when there is one.
    pub worst_case: Option<String>,
    /// Snippet for each chip, in the order of `Analysis::per_chip`.
    pub per_chip: Vec<Option<String>>,
}

/// One axis of a saved result: the analysis as computed and its snippets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AxisSnapshot {
    /// The axis analysed.
    pub axis: Axis,
    /// The analysis result.
    pub analysis: Analysis,
    /// The snippets shown with it.
    pub snippets: Snippets,
}

/// The contents of `result.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultRecord {
    /// Version of this shape; see [`RESULT_VERSION`].
    pub version: u32,
    /// Result id, also the zip's file stem.
    pub id: String,
    /// Save time as an RFC 3339 UTC string.
    pub created_at: String,
    /// Optional user label.
    pub label: Option<String>,
    /// Id of the session the result was computed from.
    pub session_id: String,
    /// `wobblescope-core` version that computed the analysis.
    pub core_version: String,
    /// Version of the daemon that wrote the result.
    pub daemon_version: String,
    /// The fit parameters the analysis used.
    pub fit_params: FitParams,
    /// Dataset overrides the user applied.
    pub overrides: Vec<String>,
    /// One snapshot per analysed axis.
    pub axes: Vec<AxisSnapshot>,
    /// SHA-256 of every other entry in the zip, as lowercase hex, keyed by entry name.
    pub manifest: BTreeMap<String, String>,
}

/// What the web app sends to save a result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct SaveRequest {
    /// Optional user label.
    pub label: Option<String>,
    /// `wobblescope-core` version that computed the axes.
    pub core_version: String,
    /// The session's `created_at`, so the daemon can tell the session changed.
    pub session_created_at: String,
    /// The fit parameters the analysis used.
    pub fit_params: FitParams,
    /// Dataset overrides the user applied.
    pub overrides: Vec<String>,
    /// One snapshot per analysed axis.
    pub axes: Vec<AxisSnapshot>,
}

/// One row of the result list. A zip that cannot be read still gets a row with `unusable` set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultSummary {
    /// Result id, taken from the file name.
    pub id: String,
    /// Optional user label.
    pub label: Option<String>,
    /// Save time, when the record could be read.
    pub created_at: Option<String>,
    /// Source session, when the record could be read.
    pub session_id: Option<String>,
    /// Core version of the analysis, when the record could be read.
    pub core_version: Option<String>,
    /// Size of the zip on disk.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size_bytes: u64,
    /// Why the zip cannot be used, when it cannot.
    pub unusable: Option<String>,
}

/// A saved result with the session metadata it carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultDetail {
    /// The record.
    pub record: ResultRecord,
    /// The session metadata from `meta.json`.
    pub meta: SessionMeta,
}
