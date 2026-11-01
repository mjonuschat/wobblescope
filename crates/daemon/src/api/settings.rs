//! `GET /api/settings`: the settings Klipper will use for a resonance test.

use axum::Json;
use axum::extract::State;
use serde_json::json;

use super::{ApiError, ApiState};
use crate::moonraker::settings::{PrinterSettings, parse_settings};

/// How long Moonraker may take to answer the settings query, which is large.
const QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub(super) async fn settings(
    State(state): State<ApiState>,
) -> Result<Json<PrinterSettings>, ApiError> {
    read(&state).await.map(Json)
}

/// Reads the settings from Moonraker now.
pub(super) async fn read(state: &ApiState) -> Result<PrinterSettings, ApiError> {
    let unavailable = |message: String| ApiError::MoonrakerUnavailable { message };
    let control = state
        .moonraker
        .control()
        .ok_or_else(|| unavailable("the daemon is not connected to Moonraker".to_owned()))?;
    let query = json!({"objects": {
        "configfile": ["settings"],
        "toolhead": ["axis_minimum", "axis_maximum"],
    }});
    let answer = control
        .call("printer.objects.query", Some(query), QUERY_TIMEOUT)
        .await
        .map_err(|e| unavailable(format!("cannot read the printer's settings: {e}")))?;
    let info = control
        .call("printer.info", None, QUERY_TIMEOUT)
        .await
        .map_err(|e| unavailable(format!("cannot read the printer's info: {e}")))?;
    parse_settings(&answer, &info).map_err(|e| unavailable(e.to_string()))
}
