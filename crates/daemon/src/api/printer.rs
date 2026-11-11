use std::time::Duration;

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use serde_json::json;

use super::tests::StopDispatched;
use super::{ApiError, ApiState};
use crate::capture::CheckFailure;
use crate::capture::preflight::read_printer_state;
use crate::capture::runner::watch_idle;
use crate::moonraker::rpc::RpcError;

/// The id the motion-uncertain marker carries after a homing request of unknown outcome.
const HOMING_MARK: &str = "homing";

const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// How long homing may take. Moonraker answers the script when the motion is done.
const HOME_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PrinterState {
    /// The axes Klipper has homed, such as `xy` or `xyz`; empty when none is.
    pub homed_axes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Homed {
    /// Always true: Klipper finished the homing move.
    pub homed: bool,
}

fn unavailable(message: impl Into<String>) -> ApiError {
    ApiError::MoonrakerUnavailable {
        message: message.into(),
    }
}

fn control(state: &ApiState) -> Result<crate::moonraker::rpc::Rpc, ApiError> {
    state
        .moonraker
        .control()
        .ok_or_else(|| unavailable("the daemon is not connected to Moonraker"))
}

pub(super) async fn printer(State(state): State<ApiState>) -> Result<Json<PrinterState>, ApiError> {
    let control = control(&state)?;
    let read = tokio::time::timeout(QUERY_TIMEOUT, read_printer_state(&control))
        .await
        .map_err(|_| unavailable("Moonraker did not answer the printer state query"))?
        .map_err(|e| unavailable(format!("cannot read the printer's state: {e}")))?;
    Ok(Json(PrinterState {
        homed_axes: read.homed_axes,
    }))
}

/// Stops the printer now. A running test is marked as stopped so its outcome says why.
pub(super) async fn emergency_stop(
    State(state): State<ApiState>,
) -> Result<Json<StopDispatched>, ApiError> {
    let slot = &state.capture.slot;
    let running = slot.request_stop_current();
    let uncertain = slot.motion_uncertain();
    let cancel = |slot: &crate::capture::slot::TestSlot| {
        if let Some(id) = &running {
            slot.cancel_stop(id);
        }
    };
    let Some(control) = state.moonraker.control() else {
        cancel(slot);
        return Err(unavailable("the daemon is not connected to Moonraker"));
    };
    if let Err(e) = control
        .call("printer.emergency_stop", None, STOP_TIMEOUT)
        .await
    {
        cancel(slot);
        return Err(unavailable(format!(
            "Moonraker did not take the emergency stop: {e}"
        )));
    }
    if let Some(id) = uncertain {
        // Klipper is shut down, so nothing keeps moving.
        slot.clear_motion_uncertain();
        state.capture.hub.update(|s| {
            if let Some(last) = s.last.as_mut().filter(|l| l.id == id) {
                last.motion_uncertain = false;
            }
        });
    }
    Ok(Json(StopDispatched { dispatched: true }))
}

/// Homes all axes. It waits for the move to finish, and is refused while a test runs or the last
/// test may still be moving the toolhead. It holds the test slot meanwhile, so no test starts
/// during the move.
pub(super) async fn home(State(state): State<ApiState>) -> Result<Json<Homed>, ApiError> {
    let Some(_permit) = state.capture.slot.try_acquire() else {
        return Err(ApiError::preflight(vec![CheckFailure::new(
            "no_test_running",
            "a test is running; the printer cannot home until it ends",
        )]));
    };
    if state.capture.slot.motion_uncertain().is_some() {
        return Err(ApiError::preflight(vec![CheckFailure::new(
            "no_uncertain_motion",
            "the printer may still be moving; stop it first",
        )]));
    }
    let control = control(&state)?;
    let result = control
        .call(
            "printer.gcode.script",
            Some(json!({"script": "G28"})),
            HOME_TIMEOUT,
        )
        .await;
    if let Err(e) = result {
        // An answer from Moonraker means G28 ended. A timeout or a lost connection does not say
        // whether the move did, so motion stays blocked until the toolhead is seen idle or the
        // user stops the printer.
        if !matches!(e, RpcError::Moonraker { .. }) {
            state.capture.slot.set_motion_uncertain(HOMING_MARK);
            watch_idle(
                state.moonraker.clone(),
                state.capture.slot.clone(),
                state.capture.hub.clone(),
                state.capture.timings,
                HOMING_MARK.to_owned(),
            );
        }
        return Err(unavailable(format!("homing failed: {e}")));
    }
    Ok(Json(Homed { homed: true }))
}
