use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wobblescope_core::fit::{Axis, FitParams};
use wobblescope_core::multichip::Analysis;
use wobblescope_session::meta::SessionMeta;

/// Version of [`ResultRecord`]. A reader refuses a newer one.
pub const RESULT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Snippets {
    pub worst_case: Option<String>,
    /// For each chip in the order of `Analysis::per_chip`: none without a recommendation, else the
    /// snippet of each shaper in that recommendation's `all`, in the same order.
    pub per_chip: Vec<Option<Vec<String>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AxisSnapshot {
    pub axis: Axis,
    pub analysis: Analysis,
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
    /// Save time, when the record could be read.
    pub created_at: Option<String>,
    /// Source session, when the record could be read.
    pub session_id: Option<String>,
    /// Core version of the analysis, when the record could be read.
    pub core_version: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub size_bytes: u64,
    /// Why the zip cannot be used, when it cannot.
    pub unusable: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResultDetail {
    pub record: ResultRecord,
    pub meta: SessionMeta,
}
