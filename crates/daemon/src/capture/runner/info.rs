//! What the printer reports about itself for the session's metadata, best effort.

use std::time::Duration;

use serde_json::{Value, json};
use wobblescope_session::meta::PrinterInfo;

use crate::moonraker::rpc::Rpc;
use crate::moonraker::settings::{Shaping, parse_shaping};

/// The printer's host name, Klipper version and the firmware of the MCUs `mcus` names (plus the
/// main MCU), best effort. A failed call leaves what it would have added empty.
pub(super) async fn printer_info(
    control: &Rpc,
    mcus: &mut Vec<String>,
    timeout: Duration,
) -> Option<PrinterInfo> {
    let info: Value = control.call("printer.info", None, timeout).await.ok()?;
    let text = |key: &str| info.get(key).and_then(Value::as_str).map(str::to_owned);
    mcus.push("mcu".to_owned());
    mcus.sort();
    mcus.dedup();
    let shaping = read_shaping(control, timeout).await;
    Some(PrinterInfo {
        name: text("hostname"),
        klipper_version: text("software_version"),
        mcu_firmware: mcu_firmware(control, mcus, timeout).await,
        square_corner_velocity: shaping.square_corner_velocity,
        damping_ratio_x: shaping.damping_ratio_x,
        damping_ratio_y: shaping.damping_ratio_y,
    })
}

/// The printer's square corner velocity and configured damping ratios, best effort: a failed
/// call leaves them unknown.
async fn read_shaping(control: &Rpc, timeout: Duration) -> Shaping {
    let query = json!({
        "toolhead": ["square_corner_velocity"],
        "configfile": ["settings"],
    });
    match control.query_objects(query, timeout).await {
        Ok(status) => parse_shaping(&status),
        Err(_) => Shaping::default(),
    }
}

/// `<mcu>: <version>` for each of `mcus`, as Klipper reports it. The main MCU is `mcu`, another
/// is `mcu <name>`. An MCU Klipper does not list is left out.
async fn mcu_firmware(control: &Rpc, mcus: &[String], timeout: Duration) -> Vec<String> {
    let key = |name: &str| {
        if name == "mcu" {
            name.to_owned()
        } else {
            format!("mcu {name}")
        }
    };
    let objects: serde_json::Map<String, Value> = mcus
        .iter()
        .map(|name| (key(name), json!(["mcu_version"])))
        .collect();
    let Ok(status) = control.query_objects(Value::Object(objects), timeout).await else {
        return Vec::new();
    };
    mcus.iter()
        .filter_map(|name| {
            let version = status.get(key(name))?.get("mcu_version")?.as_str()?;
            Some(format!("{name}: {version}"))
        })
        .collect()
}
