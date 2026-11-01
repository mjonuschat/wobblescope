//! `GET /api/status` and `GET /api/chips`: what the daemon knows about Moonraker, Klipper,
//! storage and the printer's accelerometers. Both answer 200 in every state.

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use super::ApiState;
use crate::moonraker::ChipsSnapshot;
use crate::moonraker::status::{KlippyStatus, MoonrakerStatus};

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
