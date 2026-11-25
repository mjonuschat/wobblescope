//! `GET /api/printer/motor-tuning`: every stepper with a TMC driver or a `klipper_tmc_autotune`
//! section, with the currents the driver runs at, its microsteps and the autotune settings Klipper
//! read at startup. The daemon sorts the list by stepper name.

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use serde_json::{Value, json};

use super::error::ApiPath;
use super::printer::{require_idle, require_no_uncertain_motion};
use super::{ApiError, ApiState};
use crate::capture::{CheckFailure, CheckId};
use crate::moonraker::Moonraker;
use crate::moonraker::rpc::{Rpc, RpcError};

const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

const SECTION_PREFIX: &str = "autotune_tmc ";

/// The two motors of a CoreXY printer, which are compared with each other.
const MOTOR_PAIR: [&str; 2] = ["stepper_x", "stepper_y"];
const PAIRED_KINEMATICS: [&str; 2] = ["corexy", "limited_corexy"];

/// The registers the card shows, in this order. They are the ones that set how a driver runs the
/// motor and that Shake&Tune compares. The others hold the driver's live state.
const REGISTERS: [&str; 5] = ["CHOPCONF", "PWMCONF", "COOLCONF", "TPWMTHRS", "TCOOLTHRS"];
/// How long one register read may take, and how long its answer may trail the command.
const REGISTER_TIMEOUT: Duration = Duration::from_secs(5);
const CONSOLE_GRACE: Duration = Duration::from_millis(500);

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

/// A driver register and its value as `0x` and eight hexadecimal digits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Register {
    pub name: String,
    pub value: String,
}

/// A register whose value differs from the other motor's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RegisterDifference {
    pub name: String,
    pub other: String,
}

/// The registers of one stepper's driver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MotorRegisters {
    pub stepper: String,
    /// The registers the driver answered for, in a fixed order.
    pub registers: Vec<Register>,
    /// The other motor of a CoreXY pair, on the second motor.
    pub differs_from: Option<String>,
    pub differs: Vec<RegisterDifference>,
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

/// `GET /api/printer/motor-tuning/{stepper}/registers`: reads the stepper's registers from its
/// driver with one `DUMP_TMC STEPPER=<stepper> REGISTER=<name>` each, which Klipper prints to the
/// console. On the second motor of a CoreXY pair it also reads the first motor's, to say what
/// differs. Refused while a test or a print runs.
pub(super) async fn motor_registers(
    State(state): State<ApiState>,
    ApiPath(stepper): ApiPath<String>,
) -> Result<Json<MotorRegisters>, ApiError> {
    let control = state
        .moonraker
        .control()
        .ok_or_else(|| ApiError::MoonrakerUnavailable {
            message: "the daemon is not connected to Moonraker".to_owned(),
        })?;
    let status = control
        .query_objects(json!({"configfile": ["settings"]}), QUERY_TIMEOUT)
        .await
        .map_err(|e| ApiError::MoonrakerUnavailable {
            message: format!("cannot read the printer's config: {e}"),
        })?;
    // Only a name from the config reaches a script.
    let has_driver = |name: &str| {
        status
            .pointer("/configfile/settings")
            .and_then(Value::as_object)
            .is_some_and(|settings| {
                DRIVERS
                    .iter()
                    .any(|driver| settings.contains_key(&format!("{driver} {name}")))
            })
    };
    if !has_driver(&stepper) {
        return Err(ApiError::not_found(format!(
            "no stepper {stepper} with a TMC driver"
        )));
    }
    if state.capture.slot.is_held() {
        return Err(ApiError::preflight(vec![CheckFailure::new(
            CheckId::NoTestRunning,
            "a test is running; the registers cannot be read until it ends",
        )]));
    }
    require_no_uncertain_motion(&state)?;
    require_idle(&control).await?;
    let registers = read_registers(&control, &state.moonraker, &stepper).await;
    let [first, second] = MOTOR_PAIR;
    let mut differs_from = None;
    let mut differs = Vec::new();
    if stepper == second
        && kinematics(&status).is_some_and(|k| PAIRED_KINEMATICS.contains(&k))
        && has_driver(first)
    {
        let other = read_registers(&control, &state.moonraker, first).await;
        differs = differing_registers(&registers, &other);
        differs_from = Some(first.to_owned());
    }
    Ok(Json(MotorRegisters {
        stepper,
        registers,
        differs_from,
        differs,
    }))
}

async fn read_registers(control: &Rpc, moonraker: &Moonraker, stepper: &str) -> Vec<Register> {
    let mut registers = Vec::new();
    for name in REGISTERS {
        if let Some(value) = read_register(control, moonraker, stepper, name).await {
            registers.push(Register {
                name: name.to_owned(),
                value,
            });
        }
    }
    registers
}

/// One register's value, or `None` when the driver has no such register or does not answer.
async fn read_register(
    control: &Rpc,
    moonraker: &Moonraker,
    stepper: &str,
    name: &str,
) -> Option<String> {
    // Subscribed before the command, so no line of the answer is missed.
    let mut console = moonraker.gcode_lines();
    let script = format!("DUMP_TMC STEPPER={stepper} REGISTER={name}");
    control
        .call(
            "printer.gcode.script",
            Some(json!({"script": script})),
            REGISTER_TIMEOUT,
        )
        .await
        .ok()?;
    loop {
        match tokio::time::timeout(CONSOLE_GRACE, console.recv()).await {
            Ok(Ok(line)) => {
                if let Some(value) = parse_register(&line, name) {
                    return Some(value);
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            Ok(Err(_)) | Err(_) => return None,
        }
    }
}

/// The value in a console line such as `// CHOPCONF:   330081e1 toff=1 tbl=1`, as `0x330081e1`.
fn parse_register(line: &str, name: &str) -> Option<String> {
    let rest = line
        .trim()
        .strip_prefix("//")?
        .trim_start()
        .strip_prefix(name)?
        .strip_prefix(':')?
        .trim_start();
    let digits = rest.get(..8)?;
    let ends = rest
        .get(8..)
        .is_some_and(|r| r.is_empty() || r.starts_with(' '));
    (ends && digits.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("0x{}", digits.to_ascii_lowercase()))
}

/// The registers of `a` whose value is not the one `b` has. A register one of them lacks is not
/// compared.
fn differing_registers(a: &[Register], b: &[Register]) -> Vec<RegisterDifference> {
    a.iter()
        .filter_map(|mine| {
            let theirs = b.iter().find(|r| r.name == mine.name)?;
            (theirs.value != mine.value).then(|| RegisterDifference {
                name: mine.name.clone(),
                other: theirs.value.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_register_line_gives_its_value() {
        let line = "// CHOPCONF:   330081e1 toff=1 hstrt=6 mres=3(32usteps)";
        assert_eq!(
            parse_register(line, "CHOPCONF").as_deref(),
            Some("0x330081e1")
        );
    }

    #[test]
    fn the_value_is_found_whatever_the_padding_and_the_case() {
        assert_eq!(
            parse_register("// FACTORY_CONF: 0000000D fclktrim=13", "FACTORY_CONF").as_deref(),
            Some("0x0000000d")
        );
        assert_eq!(
            parse_register("// GSTAT:      00000000", "GSTAT").as_deref(),
            Some("0x00000000")
        );
    }

    #[test]
    fn another_register_or_a_malformed_line_gives_nothing() {
        for line in [
            "// PWMCONF:   4f0e0516 pwm_ofs=22",
            "// CHOPCONF:   330081e",
            "// CHOPCONF:   330081e12 toff=1",
            "// CHOPCONF:   33008zz1 toff=1",
            "CHOPCONF:   330081e1",
            "// ========== Queried registers ==========",
            "!! Unknown register name 'CHOPCONF'",
        ] {
            assert_eq!(parse_register(line, "CHOPCONF"), None, "{line}");
        }
    }

    fn register(name: &str, value: &str) -> Register {
        Register {
            name: name.to_owned(),
            value: value.to_owned(),
        }
    }

    #[test]
    fn only_registers_both_motors_have_are_compared() {
        let a = [
            register("CHOPCONF", "0x1"),
            register("PWMCONF", "0x2"),
            register("COOLCONF", "0x3"),
        ];
        let b = [register("CHOPCONF", "0x1"), register("PWMCONF", "0x9")];
        assert_eq!(
            differing_registers(&a, &b),
            [RegisterDifference {
                name: "PWMCONF".to_owned(),
                other: "0x9".to_owned()
            }]
        );
    }
}
