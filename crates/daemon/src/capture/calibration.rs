//! What an axes-map calibration reads before it changes Klipper's settings, and the calls that
//! change and restore them.

use std::time::Duration;

use serde_json::json;

use crate::hold::settings::{CALIBRATION_STATE_NAME, Saved, restore_script_named};
use crate::moonraker::rpc::{Rpc, RpcError};

/// Reads the limits, input shaper and square corner velocity the calibration changes.
///
/// # Errors
/// A message when the query fails or a value the restore needs is not reported. The square
/// corner velocity counts as needed, because the calibration sets it to 5.
pub(super) async fn read_saved(control: &Rpc, timeout: Duration) -> Result<Saved, String> {
    let status = control
        .query_objects(
            json!({
                "toolhead": [
                    "max_velocity", "max_accel", "minimum_cruise_ratio", "square_corner_velocity",
                ],
                "input_shaper": null,
            }),
            timeout,
        )
        .await
        .map_err(|e| format!("cannot read the toolhead: {e}"))?;
    let saved = Saved::from_status(&status)?;
    if saved.square_corner_velocity.is_none() {
        return Err("the toolhead's square_corner_velocity is not reported".to_owned());
    }
    Ok(saved)
}

/// The script that saves Klipper's G-code state before the calibration changes anything.
pub(super) fn save_script() -> String {
    format!("SAVE_GCODE_STATE NAME={CALIBRATION_STATE_NAME}")
}

/// Sends `script` and waits for it.
pub(super) async fn send(control: &Rpc, script: &str, timeout: Duration) -> Result<(), RpcError> {
    control
        .call(
            "printer.gcode.script",
            Some(json!({"script": script})),
            timeout,
        )
        .await
        .map(drop)
}

/// Puts back what `saved` holds and the G-code state the calibration saved.
pub(super) async fn restore(
    control: &Rpc,
    saved: &Saved,
    timeout: Duration,
) -> Result<(), RpcError> {
    send(
        control,
        &restore_script_named(saved, CALIBRATION_STATE_NAME, true),
        timeout,
    )
    .await
}
