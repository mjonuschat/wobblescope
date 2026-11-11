//! `GET /api/settings`: the settings Klipper will use for a resonance test.

use axum::Json;
use axum::extract::State;

use super::{ApiError, ApiState};
use crate::moonraker::settings::PrinterSettings;

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
    crate::moonraker::settings::read(&control)
        .await
        .map_err(unavailable)
}
