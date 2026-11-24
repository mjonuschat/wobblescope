//! `POST /api/tests`, `POST /api/axes-map`, `POST /api/vibrations` and `POST /api/tests/{id}/emergency-stop`.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use time::OffsetDateTime;

use super::error::{ApiJson, ApiPath};
use super::{ApiError, ApiState, settings};
use crate::capture::axes_map::{AxesMapRequest, plan_axes_map};
use crate::capture::preflight::{Deps, Refusal, run};
use crate::capture::request::{TestPlan, TestRequest, plan};
use crate::capture::runner::Job;
use crate::capture::vibrations::{VibrationsRequest, plan_vibrations};

/// The answer to a started test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct StartedTest {
    /// The test id, which is also the session id.
    pub id: String,
}

/// The answer to an emergency stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct StopDispatched {
    /// Always true: Moonraker accepted the emergency stop.
    pub dispatched: bool,
}

pub(super) async fn start(
    State(state): State<ApiState>,
    ApiJson(request): ApiJson<TestRequest>,
) -> Result<(StatusCode, Json<StartedTest>), ApiError> {
    let printer = settings::read(&state).await?;
    let plan = plan(&request, &printer, state.capture.max_segment_seconds)
        .map_err(|e| ApiError::invalid(e.to_string()))?;
    start_plan(state, plan).await
}

pub(super) async fn start_axes_map(
    State(state): State<ApiState>,
    ApiJson(request): ApiJson<AxesMapRequest>,
) -> Result<(StatusCode, Json<StartedTest>), ApiError> {
    let printer = settings::read(&state).await?;
    let plan = plan_axes_map(&request, &printer, state.capture.max_segment_seconds)
        .map_err(|e| ApiError::invalid(e.to_string()))?;
    start_plan(state, plan).await
}

async fn plan_vibrations_request(
    state: &ApiState,
    request: &VibrationsRequest,
) -> Result<TestPlan, ApiError> {
    let printer = settings::read(state).await?;
    // The nominal rate of each requested chip, `None` for a chip that is not known.
    let snapshot = state.moonraker.chips();
    let rates: Vec<Option<f64>> = request
        .chips
        .iter()
        .map(|name| {
            snapshot
                .list
                .chips
                .iter()
                .find(|c| c.section == *name)
                .and_then(|c| c.nominal_rate_hz)
        })
        .collect();
    plan_vibrations(request, &printer, state.capture.max_segment_seconds, &rates)
        .map_err(|e| ApiError::invalid(e.to_string()))
}

pub(super) async fn start_vibrations(
    State(state): State<ApiState>,
    ApiJson(request): ApiJson<VibrationsRequest>,
) -> Result<(StatusCode, Json<StartedTest>), ApiError> {
    if request.angular_step.is_some() && !STEP_ENABLED {
        return Err(ApiError::invalid("angular_step is not available yet"));
    }
    let plan = plan_vibrations_request(&state, &request).await?;
    start_plan(state, plan).await
}

/// Takes the slot, runs pre-flight and spawns the runner for a plan either start handler built.
async fn start_plan(
    state: ApiState,
    plan: TestPlan,
) -> Result<(StatusCode, Json<StartedTest>), ApiError> {
    let permit = state.capture.slot.try_acquire();
    let deps = Deps::new(&state.moonraker, &state.storage, &state.capture.slot);
    let started = run(&deps, &plan, permit.is_none(), OffsetDateTime::now_utc())
        .await
        .map_err(|refusal| match refusal {
            Refusal::Checks(failures) => ApiError::preflight(failures),
            Refusal::Storage(e) => e.into(),
        })?;
    let permit = permit.ok_or_else(|| {
        ApiError::preflight(vec![crate::capture::preflight::busy_failure(
            &state.capture.slot,
            &mut Vec::new(),
        )])
    })?;
    let storage = state
        .storage
        .get()
        .cloned()
        .ok_or_else(|| ApiError::internal("storage closed during pre-flight"))?;
    let id = started.id.clone();
    // The task runs on its own; the live socket and the session list show how it ends.
    drop(
        state
            .capture
            .runner(state.moonraker.clone(), storage)
            .spawn(Job {
                started,
                plan,
                permit,
            }),
    );
    Ok((StatusCode::ACCEPTED, Json(StartedTest { id })))
}

pub(super) async fn emergency_stop(
    State(state): State<ApiState>,
    ApiPath(id): ApiPath<String>,
) -> Result<Json<StopDispatched>, ApiError> {
    let slot = &state.capture.slot;
    let running = slot.request_stop(&id);
    let uncertain = slot.motion_uncertain().as_deref() == Some(id.as_str());
    if !running && !uncertain {
        return Err(ApiError::TestNotRunning {
            message: format!("no test {id} is running or may still be moving the toolhead"),
        });
    }
    let Some(control) = state.moonraker.control() else {
        slot.cancel_stop(&id);
        return Err(ApiError::MoonrakerUnavailable {
            message: "the daemon is not connected to Moonraker".to_owned(),
        });
    };
    // No confirmation and no waiting on the runner: the call goes out at once.
    if let Err(e) = control
        .call(
            "printer.emergency_stop",
            None,
            std::time::Duration::from_secs(5),
        )
        .await
    {
        slot.cancel_stop(&id);
        return Err(ApiError::MoonrakerUnavailable {
            message: format!("Moonraker did not take the emergency stop: {e}"),
        });
    }
    if uncertain {
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
