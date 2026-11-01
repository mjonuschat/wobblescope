//! Putting the toolhead at a test point, proving it is there, and reading the window marks.
//!
//! The daemon reads the position back after the move. If an axis is off by more than the
//! tolerance it sends one corrective move aimed the other way by the error, which handles a
//! G-code offset, a mesh and a first move after homing landing low. Every script that moves
//! saves and restores the user's G-code state itself and sets absolute coordinates.

use std::time::Duration;

use serde_json::{Value, json};

use crate::moonraker::rpc::{Rpc, RpcError};

/// How far from the point an axis may be before it is corrected, in mm.
pub const TOLERANCE_MM: f64 = 0.05;

/// Time between our window start mark and the first move of Klipper's sweep, in seconds.
///
/// `TEST_RESONANCES` waits for the queue, dwells 0.5 s and then starts its accelerometer client
/// and the sweep at `get_last_move_time`, which for an idle toolhead is the estimated print time
/// plus Klipper's 0.25 s start buffer. Measured on hardware the sweep started 0.76 to 0.78 s
/// after the mark. Samples before that are a motionless lead-in, and with them the Welch
/// segments shift against the sweep, so the recorded window starts this long after the mark. It
/// is 0.05 s under the 0.75 s the constants give, so it never cuts into the sweep.
pub const SWEEP_LEAD_IN_SECONDS: f64 = 0.7;

/// The name the daemon saves the user's G-code state under.
const STATE_NAME: &str = "wobblescope";

/// How long each kind of call may take.
#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    /// A positioning script. The slowest legal move can exceed Moonraker's usual ten seconds.
    pub script: Duration,
    /// A status query.
    pub query: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            script: Duration::from_secs(120),
            query: Duration::from_secs(10),
        }
    }
}

/// Why the toolhead could not be put at the point, or a mark could not be read.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PositionError {
    /// The toolhead is not at the point even after one correction.
    #[error(
        "the toolhead cannot be put at ({}, {}, {}): it is at ({}, {}, {})",
        wanted[0], wanted[1], wanted[2], got[0], got[1], got[2]
    )]
    Unreachable {
        /// The point asked for.
        wanted: [f64; 3],
        /// Where the toolhead reports it is.
        got: [f64; 3],
    },
    /// Klipper refused a command, with its message. The toolhead did not move on it.
    #[error("Klipper refused the move: {0}")]
    Klipper(String),
    /// A call timed out or the connection dropped, so whether the toolhead moved is unknown.
    #[error("the outcome of the move is unknown: {0}")]
    Unknown(String),
}

fn script(target: [f64; 3], speed: f64) -> String {
    let [x, y, z] = target;
    format!(
        "SAVE_GCODE_STATE NAME={STATE_NAME}\nG90\nG1 X{x} Y{y} Z{z} F{}\nM400\n\
         RESTORE_GCODE_STATE NAME={STATE_NAME}",
        speed * 60.0
    )
}

async fn query(control: &Rpc, objects: &Value, timeout: Duration) -> Result<Value, PositionError> {
    control
        .call(
            "printer.objects.query",
            Some(json!({"objects": objects})),
            timeout,
        )
        .await
        .map_err(|e| PositionError::Unknown(e.to_string()))
}

fn numbers<const N: usize>(value: &Value) -> Option<[f64; N]> {
    let list = value.as_array()?;
    let mut out = [0.0; N];
    for (slot, v) in out.iter_mut().zip(list) {
        *slot = v.as_f64()?;
    }
    (list.len() >= N).then_some(out)
}

async fn position(control: &Rpc, timeout: Duration) -> Result<[f64; 3], PositionError> {
    let answer = query(control, &json!({"toolhead": ["position"]}), timeout).await?;
    answer
        .get("status")
        .and_then(|s| s.get("toolhead"))
        .and_then(|t| t.get("position"))
        .and_then(numbers::<3>)
        .ok_or_else(|| PositionError::Unknown("the toolhead position is not reported".to_owned()))
}

/// Runs one positioning script. A G-code error while Klipper is still ready is followed by a
/// separate restore of the saved state, because the script aborts at its first error.
async fn run(
    control: &Rpc,
    text: String,
    timeouts: Timeouts,
    ready: &impl Fn() -> bool,
) -> Result<(), PositionError> {
    match control
        .call(
            "printer.gcode.script",
            Some(json!({"script": text})),
            timeouts.script,
        )
        .await
    {
        Ok(_) => Ok(()),
        Err(RpcError::Moonraker { message, .. }) => {
            if ready() {
                let restore = format!("RESTORE_GCODE_STATE NAME={STATE_NAME}");
                let _ = control
                    .call(
                        "printer.gcode.script",
                        Some(json!({"script": restore})),
                        timeouts.query,
                    )
                    .await;
            }
            Err(PositionError::Klipper(message))
        }
        Err(e) => Err(PositionError::Unknown(e.to_string())),
    }
}

/// Moves the toolhead to `point` at `speed` mm/s and proves it arrived. `ready` says whether
/// Klipper is still ready, which decides whether a failed move is cleaned up.
///
/// # Errors
/// [`PositionError`]: the point cannot be reached, Klipper refused, or the outcome is unknown.
pub async fn move_to_point(
    control: &Rpc,
    point: [f64; 3],
    speed: f64,
    timeouts: Timeouts,
    ready: impl Fn() -> bool,
) -> Result<(), PositionError> {
    run(control, script(point, speed), timeouts, &ready).await?;
    let got = position(control, timeouts.query).await?;
    let error = [got[0] - point[0], got[1] - point[1], got[2] - point[2]];
    if error.iter().all(|e| e.abs() <= TOLERANCE_MM) {
        return Ok(());
    }
    let target = [
        point[0] - error[0],
        point[1] - error[1],
        point[2] - error[2],
    ];
    run(control, script(target, speed), timeouts, &ready).await?;
    let got = position(control, timeouts.query).await?;
    if (0..3).all(|i| (got[i] - point[i]).abs() <= TOLERANCE_MM) {
        Ok(())
    } else {
        Err(PositionError::Unreachable { wanted: point, got })
    }
}

async fn print_times(control: &Rpc, timeout: Duration) -> Result<(f64, f64), PositionError> {
    let answer = query(
        control,
        &json!({"toolhead": ["print_time", "estimated_print_time"]}),
        timeout,
    )
    .await?;
    let toolhead = answer.get("status").and_then(|s| s.get("toolhead"));
    let read = |key: &str| {
        toolhead
            .and_then(|t| t.get(key))
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
    };
    read("print_time")
        .zip(read("estimated_print_time"))
        .ok_or_else(|| {
            PositionError::Unknown("the toolhead's print time is not reported".to_owned())
        })
}

/// The window start: the larger of `print_time` and `estimated_print_time`, plus
/// [`SWEEP_LEAD_IN_SECONDS`]. After a zero-length move `print_time` can lag the sample clock by
/// many seconds, and `estimated_print_time` is never behind the real end of the move.
///
/// # Errors
/// [`PositionError::Unknown`] when the answer fails or lacks the times.
pub async fn window_start(control: &Rpc, timeout: Duration) -> Result<f64, PositionError> {
    let (print_time, estimated) = print_times(control, timeout).await?;
    Ok(print_time.max(estimated) + SWEEP_LEAD_IN_SECONDS)
}

/// The window end: `print_time`, read after the sweep's `M400`.
///
/// # Errors
/// [`PositionError::Unknown`] when the answer fails or lacks the time.
pub async fn window_end(control: &Rpc, timeout: Duration) -> Result<f64, PositionError> {
    Ok(print_times(control, timeout).await?.0)
}
