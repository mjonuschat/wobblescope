//! `GET /api/printer/motor-tuning`: every stepper with a TMC driver or a `klipper_tmc_autotune`
//! section, with the currents the driver runs at, its microsteps and the autotune settings Klipper
//! read at startup. The daemon sorts the list by stepper name.

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use serde_json::{Value, json};

use super::{ApiError, ApiState};
use crate::moonraker::rpc::RpcError;

const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

const SECTION_PREFIX: &str = "autotune_tmc ";

/// The two motors of a CoreXY printer, which are compared with each other.
const MOTOR_PAIR: [&str; 2] = ["stepper_x", "stepper_y"];
const PAIRED_KINEMATICS: [&str; 2] = ["corexy", "limited_corexy"];

const DRIVERS: [&str; 6] = [
    "tmc2130", "tmc2208", "tmc2209", "tmc2240", "tmc2660", "tmc5160",
];

/// One stepper: its driver, the currents it runs at, its microsteps and its `[autotune_tmc]`
/// section with the defaults filled in. A value the stepper does not have is null.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MotorTuning {
    pub stepper: String,
    /// The driver type of the stepper's driver section, such as `tmc2240`; null when there is none.
    pub driver: Option<String>,
    /// The current in amperes the driver runs the motor at, to two decimals.
    pub run_current: Option<f64>,
    /// The current in amperes the driver holds the motor at, to two decimals.
    pub hold_current: Option<f64>,
    pub microsteps: Option<u32>,
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
    /// The other motor of a CoreXY pair, on the second motor when their configurations differ.
    pub differs_from: Option<String>,
    /// The names of the fields that differ from `differs_from`.
    pub differs: Vec<String>,
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
    let status = match control.query_objects(query, QUERY_TIMEOUT).await {
        Ok(status) => status,
        // Klipper answers with an error while it is not ready.
        Err(RpcError::Moonraker { .. }) => return Ok(Json(Vec::new())),
        Err(e) => {
            return Err(unavailable(format!(
                "cannot read the printer's config: {e}"
            )));
        }
    };
    let mut list = parse(&status);
    let drivers: serde_json::Map<String, Value> = list
        .iter()
        .filter_map(|m| {
            Some((
                format!("{} {}", m.driver.as_ref()?, m.stepper),
                json!(CURRENT_FIELDS),
            ))
        })
        .collect();
    if !drivers.is_empty() {
        // The currents in use are not in the config, which holds the driver maximum as the hold
        // current by default. Without them the rest of the list is still worth showing.
        if let Ok(currents) = control
            .query_objects(Value::Object(drivers), QUERY_TIMEOUT)
            .await
        {
            apply_currents(&mut list, &currents);
        }
    }
    mark_differences(&mut list, kinematics(&status));
    Ok(Json(list))
}

const CURRENT_FIELDS: [&str; 2] = ["run_current", "hold_current"];

/// A current in amperes, rounded to two decimals.
fn amperes(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn kinematics(status: &Value) -> Option<&str> {
    status
        .pointer("/configfile/settings/printer/kinematics")
        .and_then(Value::as_str)
}

fn parse(status: &Value) -> Vec<MotorTuning> {
    let Some(settings) = status
        .pointer("/configfile/settings")
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let mut steppers: Vec<&str> = settings
        .keys()
        .filter_map(|section| {
            section.strip_prefix(SECTION_PREFIX).or_else(|| {
                DRIVERS
                    .iter()
                    .find_map(|driver| section.strip_prefix(driver)?.strip_prefix(' '))
            })
        })
        .map(str::trim)
        .collect();
    steppers.sort_unstable();
    steppers.dedup();
    steppers
        .into_iter()
        .map(|stepper| {
            let autotune = settings.get(&format!("{SECTION_PREFIX}{stepper}"));
            let text = |key: &str| {
                autotune
                    .and_then(|o| o.get(key))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            };
            let number = |key: &str| autotune.and_then(|o| o.get(key)).and_then(Value::as_f64);
            let driver = DRIVERS
                .iter()
                .find(|driver| settings.contains_key(&format!("{driver} {stepper}")));
            MotorTuning {
                stepper: stepper.to_owned(),
                driver: driver.map(|d| (*d).to_owned()),
                run_current: None,
                hold_current: None,
                microsteps: settings
                    .get(stepper)
                    .and_then(|o| o.get("microsteps"))
                    .and_then(Value::as_u64)
                    .and_then(|m| u32::try_from(m).ok()),
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
                differs_from: None,
                differs: Vec::new(),
            }
        })
        .collect()
}

fn apply_currents(list: &mut [MotorTuning], status: &Value) {
    for motor in list {
        let Some(driver) = &motor.driver else {
            continue;
        };
        let Some(object) = status.get(format!("{driver} {}", motor.stepper)) else {
            continue;
        };
        let current = |key: &str| object.get(key).and_then(Value::as_f64).map(amperes);
        motor.run_current = current("run_current");
        motor.hold_current = current("hold_current");
    }
}

/// The fields of two steppers that differ, in the order the card shows them.
fn differing(a: &MotorTuning, b: &MotorTuning) -> Vec<String> {
    let checks: [(&str, bool); 20] = [
        ("driver", a.driver != b.driver),
        ("motor", a.motor != b.motor),
        ("tuning_goal", a.tuning_goal != b.tuning_goal),
        ("run_current", a.run_current != b.run_current),
        ("hold_current", a.hold_current != b.hold_current),
        ("microsteps", a.microsteps != b.microsteps),
        ("voltage", a.voltage != b.voltage),
        ("tbl", a.tbl != b.tbl),
        ("toff", a.toff != b.toff),
        ("tpfd", a.tpfd != b.tpfd),
        ("sgt", a.sgt != b.sgt),
        ("sg4_thrs", a.sg4_thrs != b.sg4_thrs),
        ("extra_hysteresis", a.extra_hysteresis != b.extra_hysteresis),
        ("semin", a.semin != b.semin),
        ("semax", a.semax != b.semax),
        ("seup", a.seup != b.seup),
        ("sedn", a.sedn != b.sedn),
        ("seimin", a.seimin != b.seimin),
        ("overvoltage_vth", a.overvoltage_vth != b.overvoltage_vth),
        ("pwm_freq_target", a.pwm_freq_target != b.pwm_freq_target),
    ];
    checks
        .into_iter()
        .filter(|(_, differs)| *differs)
        .map(|(key, _)| key.to_owned())
        .collect()
}

/// On a CoreXY printer the two motors drive the diagonals, so a vibrations run reads both. Marks
/// what differs on the second one.
fn mark_differences(list: &mut [MotorTuning], kinematics: Option<&str>) {
    if !kinematics.is_some_and(|k| PAIRED_KINEMATICS.contains(&k)) {
        return;
    }
    let [first, second] = MOTOR_PAIR;
    let (Some(a), Some(b)) = (
        list.iter().position(|m| m.stepper == first),
        list.iter().position(|m| m.stepper == second),
    ) else {
        return;
    };
    let differs = differing(&list[a], &list[b]);
    if !differs.is_empty() {
        list[b].differs_from = Some(first.to_owned());
        list[b].differs = differs;
    }
}
