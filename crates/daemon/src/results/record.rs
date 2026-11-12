use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wobblescope_core::fit::{Axis, FitParams};
use wobblescope_core::multichip::Analysis;
use wobblescope_session::meta::SessionMeta;

/// Version of [`ResultRecord`]. A reader refuses a newer one.
///
/// Version 2 results come from a core that can estimate the damping ratio, so their
/// `fit_params.damping_ratio` of `None` means "estimate". In a version 1 record a missing or zero
/// ratio meant 0.1; see [`FIRST_ESTIMATING_VERSION`].
pub const RESULT_VERSION: u32 = 2;

/// The first [`ResultRecord`] version whose `fit_params` can ask for an estimated damping ratio.
/// An older record's `fit_params` is read with `FitParams::from_legacy` when it is opened.
pub const FIRST_ESTIMATING_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Snippets {
    pub worst_case: Option<String>,
    /// For each chip in the order of `Analysis::per_chip`: none without a recommendation, else the
    /// snippet of each shaper in that recommendation's `all`, in the same order.
    pub per_chip: Vec<Option<Vec<String>>>,
    /// The snippet of the worst-case recommendation's "for performance" shaper, when it has one.
    #[serde(default)]
    pub worst_case_performance: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AxisSnapshot {
    pub axis: Axis,
    pub analysis: Analysis,
    pub snippets: Snippets,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultRecord {
    /// Version of this shape; see [`RESULT_VERSION`].
    pub version: u32,
    /// Result id, also the zip's file stem.
    pub id: String,
    /// Save time as an RFC 3339 UTC string.
    pub created_at: String,
    pub label: Option<String>,
    pub session_id: String,
    pub core_version: String,
    pub daemon_version: String,
    pub fit_params: FitParams,
    pub overrides: Vec<String>,
    pub axes: Vec<AxisSnapshot>,
    /// SHA-256 of every other entry in the zip, as lowercase hex, keyed by entry name.
    pub manifest: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(deny_unknown_fields)]
pub struct SaveRequest {
    pub label: Option<String>,
    pub core_version: String,
    /// The session's `created_at`, so the daemon can tell the session changed.
    pub session_created_at: String,
    pub fit_params: FitParams,
    pub overrides: Vec<String>,
    pub axes: Vec<AxisSnapshot>,
}

/// One row of the result list. A zip that cannot be read still gets a row with `unusable` set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultSummary {
    /// Result id, taken from the file name.
    pub id: String,
    pub label: Option<String>,
    pub created_at: Option<String>,
    pub session_id: Option<String>,
    pub core_version: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size_bytes: u64,
    pub unusable: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultDetail {
    pub record: ResultRecord,
    pub meta: SessionMeta,
}
