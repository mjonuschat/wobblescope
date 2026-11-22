//! `GET /api/printer/motor-tuning`: the `klipper_tmc_autotune` settings Klipper read at startup.
//! The list is sorted by stepper name.

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use serde_json::{Value, json};

use super::{ApiError, ApiState};
use crate::moonraker::rpc::RpcError;

const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

const SECTION_PREFIX: &str = "autotune_tmc ";

const DRIVERS: [&str; 6] = [
    "tmc2130", "tmc2208", "tmc2209", "tmc2240", "tmc2660", "tmc5160",
];

/// One `[autotune_tmc <stepper>]` section with its defaults filled in. An option the section
/// does not have is null.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MotorTuning {
    pub stepper: String,
    /// The driver type of the stepper's driver section, such as `tmc2240`; null when there is none.
    pub driver: Option<String>,
    pub motor: Option<String>,
    pub tuning_goal: Option<String>,
    pub extra_hysteresis: Option<f64>,
    pub tbl: Option<f64>,
    pub toff: Option<f64>,
    pub tpfd: Option<f64>,
    pub sgt: Option<f64>,
    pub sg4_thrs: Option<f64>,
    pub semin: Option<f64>,
    pub semax: Option<f64>,
    pub seup: Option<f64>,
    pub sedn: Option<f64>,
    pub seimin: Option<f64>,
    pub voltage: Option<f64>,
    pub overvoltage_vth: Option<f64>,
    pub pwm_freq_target: Option<f64>,
}

pub(super) async fn motor_tuning(
    State(state): State<ApiState>,
) -> Result<Json<Vec<MotorTuning>>, ApiError> {
    let unavailable = |message: String| ApiError::MoonrakerUnavailable { message };
    let control = state
        .moonraker
        .control()
        .ok_or_else(|| unavailable("the daemon is not connected to Moonraker".to_owned()))?;
    let query = json!({"configfile": ["settings"]});
    match control.query_objects(query, QUERY_TIMEOUT).await {
        Ok(status) => Ok(Json(parse(&status))),
        // Klipper answers with an error while it is not ready.
        Err(RpcError::Moonraker { .. }) => Ok(Json(Vec::new())),
        Err(e) => Err(unavailable(format!(
            "cannot read the printer's config: {e}"
        ))),
    }
}

fn parse(status: &Value) -> Vec<MotorTuning> {
    let Some(settings) = status
        .pointer("/configfile/settings")
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let mut list: Vec<MotorTuning> = settings
        .iter()
        .filter_map(|(section, options)| {
            let stepper = section.strip_prefix(SECTION_PREFIX)?.trim();
            let text = |key: &str| options.get(key).and_then(Value::as_str).map(str::to_owned);
            let number = |key: &str| options.get(key).and_then(Value::as_f64);
            let driver = DRIVERS
                .iter()
                .find(|driver| settings.contains_key(&format!("{driver} {stepper}")));
            Some(MotorTuning {
                stepper: stepper.to_owned(),
                driver: driver.map(|d| (*d).to_owned()),
                motor: text("motor"),
                tuning_goal: text("tuning_goal"),
                extra_hysteresis: number("extra_hysteresis"),
                tbl: number("tbl"),
                toff: number("toff"),
                tpfd: number("tpfd"),
                sgt: number("sgt"),
                sg4_thrs: number("sg4_thrs"),
                semin: number("semin"),
                semax: number("semax"),
                seup: number("seup"),
                sedn: number("sedn"),
                seimin: number("seimin"),
                voltage: number("voltage"),
                overvoltage_vth: number("overvoltage_vth"),
                pwm_freq_target: number("pwm_freq_target"),
            })
        })
        .collect();
    list.sort_by(|a, b| a.stepper.cmp(&b.stepper));
    list
}
