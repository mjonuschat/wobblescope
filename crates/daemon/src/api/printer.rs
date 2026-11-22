use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::tests::StopDispatched;
use super::{ApiError, ApiState};
use crate::capture::preflight::quarantine_duration;
use crate::capture::preflight::read_printer_state;
use crate::capture::runner::watch_idle;
use crate::capture::slot::{CHIP_CHECK_ID, HOMING_ID, LEVELLING_ID};
use crate::capture::{CheckFailure, CheckId};
use crate::moonraker::rpc::{Rpc, RpcError};

const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// How long homing may take. Moonraker answers the script when the motion is done.
const HOME_TIMEOUT: Duration = Duration::from_secs(300);

/// How long levelling may take. It probes several points, and may repeat until the probed range
/// is within tolerance.
const LEVEL_TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum LevelKind {
    /// `[z_tilt]`, or Kalico's `[z_tilt_ng]`, run with `Z_TILT_ADJUST`.
    ZTilt,
    /// `[quad_gantry_level]`, run with `QUAD_GANTRY_LEVEL`.
    QuadGantryLevel,
}

impl LevelKind {
    fn command(self) -> &'static str {
        match self {
            Self::ZTilt => "Z_TILT_ADJUST",
            Self::QuadGantryLevel => "QUAD_GANTRY_LEVEL",
        }
    }
}

/// A levelling procedure and the Klipper object that reports its `applied` flag. Kalico's
/// `[z_tilt_ng]` is a second object for Z-tilt, so a printer has one of the two.
#[derive(Debug, Clone, Copy)]
struct Levelling {
    kind: LevelKind,
    object: &'static str,
}

const LEVELLING_OBJECTS: [Levelling; 3] = [
    Levelling {
        kind: LevelKind::ZTilt,
        object: "z_tilt",
    },
    Levelling {
        kind: LevelKind::ZTilt,
        object: "z_tilt_ng",
    },
    Levelling {
        kind: LevelKind::QuadGantryLevel,
        object: "quad_gantry_level",
    },
];

fn level_command(state: &ApiState, kind: LevelKind) -> String {
    (state.settings.level_command.as_deref())
        .map_or_else(|| kind.command().to_owned(), str::to_owned)
}

async fn detect_levelling(control: &Rpc) -> Result<Option<Levelling>, RpcError> {
    let answer = control
        .call("printer.objects.list", None, QUERY_TIMEOUT)
        .await?;
    let has = |name: &str| {
        answer["objects"]
            .as_array()
            .is_some_and(|list| list.iter().any(|o| o.as_str() == Some(name)))
    };
    Ok(LEVELLING_OBJECTS
        .into_iter()
        .find(|levelling| has(levelling.object)))
}

/// The answer of `query`, or `None` when it fails or takes longer than the query timeout.
async fn best_effort<T>(query: impl Future<Output = Result<T, RpcError>>) -> Option<T> {
    tokio::time::timeout(QUERY_TIMEOUT, query)
        .await
        .ok()
        .and_then(Result::ok)
}

/// Whether Klipper reports the printer as levelled. `applied` is true only after a levelling run
/// finished within its tolerance. Klipper clears it when a run starts (even one that then fails
/// or is stopped) and when the steppers are disabled (`M84`, the idle timeout, a failed homing).
/// Homing, including `G28 Z`, does not clear it, so it can stay true after Z is homed again.
/// Kalico's `z_tilt_ng` keeps the same flag and clears it in the same cases.
async fn read_levelled(control: &Rpc, levelling: Levelling) -> Result<Option<bool>, RpcError> {
    let status = control
        .query_objects(json!({ levelling.object: ["applied"] }), QUERY_TIMEOUT)
        .await?;
    Ok(status[levelling.object]["applied"].as_bool())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PrinterState {
    /// The axes Klipper has homed, such as `xy` or `xyz`; empty when none is.
    pub homed_axes: Option<String>,
    /// Why the printer cannot take a test right now, or null when it can.
    pub busy: Option<String>,
    /// Whether the printer is levelled, or null when it has no levelling.
    pub levelled: Option<bool>,
    /// The levelling the printer has, or null when it has none.
    pub level_kind: Option<LevelKind>,
    /// The G-code command the level button runs: `level_command` from the config, else the
    /// levelling's own command. Null when the printer has no levelling.
    pub level_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Levelled {
    /// Always true: Klipper finished the levelling run.
    pub levelled: bool,
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
    let slot = &state.capture.slot;
    let quarantined = slot.quarantine_remaining().is_some();
    let busy = match slot.current_id() {
        Some(id) if id == HOMING_ID => Some("The printer is homing."),
        Some(id) if id == LEVELLING_ID => Some("The printer is levelling."),
        Some(id) if id == CHIP_CHECK_ID => Some("The chips are being checked."),
        Some(id) if id.starts_with("hold-") => Some("A hold is running."),
        Some(_) => Some("A test is running."),
        None if slot.is_held() => Some("A test is running."),
        None if !read.idle(quarantined) => Some("The toolhead is moving."),
        None if quarantined || slot.motion_uncertain().is_some() => {
            Some("The printer may still be moving.")
        }
        None => None,
    };
    // Levelling is extra information. When it cannot be read, the rest of the state still counts.
    let levelling = best_effort(detect_levelling(&control)).await.flatten();
    let level_kind = levelling.map(|l| l.kind);
    let levelled = match levelling {
        Some(levelling) => best_effort(read_levelled(&control, levelling))
            .await
            .flatten(),
        None => None,
    };
    Ok(Json(PrinterState {
        homed_axes: read.homed_axes,
        busy: busy.map(str::to_owned),
        levelled: level_kind.map(|_| levelled.unwrap_or(false)),
        level_command: level_kind.map(|kind| level_command(&state, kind)),
        level_kind,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum HomeAxes {
    #[default]
    All,
    Xy,
    X,
    Y,
    Z,
}

impl HomeAxes {
    fn gcode(self) -> &'static str {
        match self {
            Self::All => "G28",
            Self::Xy => "G28 X Y",
            Self::X => "G28 X",
            Self::Y => "G28 Y",
            Self::Z => "G28 Z",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HomeRequest {
    #[serde(default)]
    axes: HomeAxes,
}

/// An empty body means all axes.
fn parse_home_axes(body: &Bytes) -> Result<HomeAxes, ApiError> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(HomeAxes::All);
    }
    serde_json::from_slice::<HomeRequest>(body)
        .map(|r| r.axes)
        .map_err(|e| ApiError::invalid(format!("invalid home request: {e}")))
}

/// Homes the requested axes, all by default. It waits for the move to finish, and is refused while
/// a test runs or the last test may still be moving the toolhead. It holds the test slot
/// meanwhile, so no test starts during the move.
pub(super) async fn home(
    State(state): State<ApiState>,
    body: Bytes,
) -> Result<Json<Homed>, ApiError> {
    let axes = parse_home_axes(&body)?;
    move_printer(&state, HOMING_ID, axes.gcode(), HOME_TIMEOUT, "homing").await?;
    Ok(Json(Homed { homed: true }))
}

/// Levels the gantry or bed with the printer's own procedure, or with `level_command`. Refused
/// and guarded like homing, with a longer timeout.
pub(super) async fn level(State(state): State<ApiState>) -> Result<Json<Levelled>, ApiError> {
    let control = control(&state)?;
    let levelling = tokio::time::timeout(QUERY_TIMEOUT, detect_levelling(&control))
        .await
        .map_err(|_| unavailable("Moonraker did not answer the object list query"))?
        .map_err(|e| unavailable(format!("cannot read the printer's objects: {e}")))?
        .ok_or_else(|| {
            ApiError::invalid("this printer has no z_tilt, z_tilt_ng or quad_gantry_level")
        })?;
    let command = level_command(&state, levelling.kind);
    move_printer(&state, LEVELLING_ID, &command, LEVEL_TIMEOUT, "levelling").await?;
    Ok(Json(Levelled { levelled: true }))
}

async fn move_printer(
    state: &ApiState,
    id: &'static str,
    script: &str,
    timeout: Duration,
    what: &str,
) -> Result<(), ApiError> {
    let Some(_permit) = state.capture.slot.try_acquire_as(id) else {
        return Err(ApiError::preflight(vec![CheckFailure::new(
            CheckId::NoTestRunning,
            "a test is running; the printer cannot move until it ends",
        )]));
    };
    if state.capture.slot.motion_uncertain().is_some() {
        return Err(ApiError::preflight(vec![CheckFailure::new(
            CheckId::NoUncertainMotion,
            "the printer may still be moving; stop it first",
        )]));
    }
    let control = control(state)?;
    // A print, or a macro that Klipper is still running after the daemon stopped waiting for it,
    // keeps Klipper out of idle, so motion is refused without relying on the marker alone. A
    // missing idle state counts as not idle. The check and the script are separate calls, so a
    // client that starts a print between them is not stopped; Klipper has no atomic guard for
    // that without a macro.
    let read = tokio::time::timeout(QUERY_TIMEOUT, read_printer_state(&control))
        .await
        .map_err(|_| unavailable("Moonraker did not answer the printer state query"))?
        .map_err(|e| unavailable(format!("cannot read the printer's state: {e}")))?;
    let mut failures = read.failures(true);
    failures.retain(|f| f.check != CheckId::Homed);
    if !failures.is_empty() {
        return Err(ApiError::preflight(failures));
    }
    let result = control
        .call(
            "printer.gcode.script",
            Some(json!({"script": script})),
            timeout,
        )
        .await;
    if let Err(e) = result {
        // An answer from Moonraker means the script ended. A timeout or a lost connection does
        // not say whether the move did, so motion stays blocked until the toolhead is seen idle
        // or the user stops the printer.
        if !matches!(e, RpcError::Moonraker { .. }) {
            state.capture.slot.set_motion_uncertain(id);
            // The quarantine is what keeps test admission from treating the printer as idle.
            state
                .capture
                .slot
                .start_quarantine(quarantine_duration(state.capture.max_segment_seconds));
            watch_idle(
                state.moonraker.clone(),
                state.capture.slot.clone(),
                state.capture.hub.clone(),
                state.capture.timings,
                id.to_owned(),
            );
        }
        return Err(unavailable(format!("{what} failed: {e}")));
    }
    Ok(())
}
