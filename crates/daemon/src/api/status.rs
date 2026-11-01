//! `GET /api/status` and `GET /api/chips`: what the daemon knows about Moonraker, Klipper,
//! storage and the printer's accelerometers. Both answer 200 in every state.

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use time::OffsetDateTime;

use super::{ApiError, ApiState};
use crate::capture::CheckFailure;
use crate::capture::availability::{AVAILABILITY_TIMEOUT, availability_of, open_chips};
use crate::moonraker::ChipsSnapshot;
use crate::moonraker::status::{KlippyState, KlippyStatus, MoonrakerState, MoonrakerStatus};

/// Whether storage is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum StorageState {
    /// The daemon is waiting for Moonraker to report the data directory.
    Waiting,
    /// Storage is open.
    Ready,
}

/// The storage part of the status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct StorageStatus {
    /// Whether storage is open.
    pub state: StorageState,
}

/// The body of `GET /api/status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct StatusResponse {
    /// The connection to Moonraker.
    pub moonraker: MoonrakerStatus,
    /// Klipper behind it.
    pub klippy: KlippyStatus,
    /// Whether sessions can be stored and listed.
    pub storage: StorageStatus,
}

pub(super) async fn status(State(state): State<ApiState>) -> Json<StatusResponse> {
    let status = state.moonraker.status();
    let storage = if state.storage.get().is_some() {
        StorageState::Ready
    } else {
        StorageState::Waiting
    };
    Json(StatusResponse {
        moonraker: status.moonraker,
        klippy: status.klippy,
        storage: StorageStatus { state: storage },
    })
}

pub(super) async fn chips(State(state): State<ApiState>) -> Json<ChipsSnapshot> {
    Json(state.moonraker.chips())
}

/// `POST /api/chips/check`: opens every configured chip, waits for valid samples, records what
/// each one did and closes the connections again. The answer is the updated chip list.
pub(super) async fn check_chips(
    State(state): State<ApiState>,
) -> Result<Json<ChipsSnapshot>, ApiError> {
    let status = state.moonraker.status();
    let mut failures = Vec::new();
    if status.moonraker.state != MoonrakerState::Connected {
        failures.push(CheckFailure::new(
            "moonraker_connected",
            "the daemon is not connected to Moonraker",
        ));
    } else if status.klippy.state != KlippyState::Ready {
        failures.push(CheckFailure::new(
            "klipper_ready",
            format!(
                "Klipper is {}, not ready; chips can be checked once it is",
                status.klippy.state.as_str()
            ),
        ));
    }
    if !failures.is_empty() {
        return Err(ApiError::preflight(failures));
    }
    let chips = state.moonraker.chips().list.chips;
    let outcomes = open_chips(&state.moonraker, &chips, AVAILABILITY_TIMEOUT)
        .await
        .map_err(|e| {
            ApiError::preflight(vec![CheckFailure::new(
                "moonraker_connected",
                e.to_string(),
            )])
        })?;
    let now = OffsetDateTime::now_utc();
    let results = outcomes
        .iter()
        .map(|(chip, outcome)| (chip.section.clone(), availability_of(outcome, now)))
        .collect();
    // The connections close when `outcomes` drops, after the results are recorded.
    state.moonraker.set_availability(results);
    drop(outcomes);
    Ok(Json(state.moonraker.chips()))
}
