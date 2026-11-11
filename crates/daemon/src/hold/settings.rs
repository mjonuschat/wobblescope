//! What a hold changes in Klipper, and the scripts that set it and put it back.

use serde::Serialize;
use serde_json::Value;

use crate::hold::pieces::Drive;

/// The name the hold saves Klipper's G-code state under.
pub const STATE_NAME: &str = "wobblescope_hold";

/// The input shaper's settings on both axes.
#[derive(Debug, Clone, PartialEq)]
pub struct Shaper {
    /// The shaper type on X.
    pub type_x: String,
    /// The shaper frequency on X, in Hz.
    pub freq_x: f64,
    /// The damping ratio on X.
    pub damping_x: f64,
    /// The shaper type on Y.
    pub type_y: String,
    /// The shaper frequency on Y, in Hz.
    pub freq_y: f64,
    /// The damping ratio on Y.
    pub damping_y: f64,
}

/// What the printer was set to before the hold changed it.
#[derive(Debug, Clone, PartialEq)]
pub struct Saved {
    /// The velocity limit, in mm/s.
    pub max_velocity: f64,
    /// The acceleration limit, in mm/s².
    pub max_accel: f64,
    /// The minimum cruise ratio, when Klipper reports one.
    pub minimum_cruise_ratio: Option<f64>,
    /// The input shaper, when the printer has one.
    pub shaper: Option<Shaper>,
}

/// A number as G-code text, without a trailing `.0`.
fn num(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        v.to_string()
    }
}

fn number(object: &Value, key: &str) -> Option<f64> {
    object.get(key).and_then(Value::as_f64)
}

impl Saved {
    /// Reads the `status` object of a `printer.objects.query` answer for `toolhead` and
    /// `input_shaper`. A missing or empty `input_shaper` means the printer has none.
    ///
    /// # Errors
    /// A message naming the missing toolhead field.
    pub fn from_status(status: &Value) -> Result<Self, String> {
        let toolhead = status.get("toolhead").unwrap_or(&Value::Null);
        let need = |key: &str| {
            number(toolhead, key).ok_or_else(|| format!("the toolhead's {key} is not reported"))
        };
        let shaper = status.get("input_shaper").and_then(|s| {
            let text = |key: &str| s.get(key).and_then(Value::as_str).map(str::to_owned);
            Some(Shaper {
                type_x: text("shaper_type_x")?,
                freq_x: number(s, "shaper_freq_x")?,
                damping_x: number(s, "damping_ratio_x")?,
                type_y: text("shaper_type_y")?,
                freq_y: number(s, "shaper_freq_y")?,
                damping_y: number(s, "damping_ratio_y")?,
            })
        });
        Ok(Self {
            max_velocity: need("max_velocity")?,
            max_accel: need("max_accel")?,
            minimum_cruise_ratio: number(toolhead, "minimum_cruise_ratio"),
            shaper,
        })
    }
}

/// The script that raises the acceleration limit for `drive`, sets the cruise ratio to 0 where
/// Klipper reports one and turns input shaping off where the printer has it.
#[must_use]
pub fn setup_script(saved: &Saved, drive: &Drive) -> String {
    let mut limit = accel_line(drive);
    if saved.minimum_cruise_ratio.is_some() {
        limit.push_str(" MINIMUM_CRUISE_RATIO=0");
    }
    let mut lines = vec![limit];
    if saved.shaper.is_some() {
        lines.push("SET_INPUT_SHAPER SHAPER_FREQ_X=0 SHAPER_FREQ_Y=0".to_owned());
    }
    lines.join("\n")
}

/// The line that sets the acceleration limit for `drive`'s current frequency.
#[must_use]
pub fn accel_line(drive: &Drive) -> String {
    format!("SET_VELOCITY_LIMIT ACCEL={}", num(drive.acceleration()))
}

/// The script that puts the saved limits, input shaping and G-code state back.
#[must_use]
pub fn restore_script(saved: &Saved) -> String {
    let mut limit = format!(
        "SET_VELOCITY_LIMIT VELOCITY={} ACCEL={}",
        num(saved.max_velocity),
        num(saved.max_accel)
    );
    if let Some(ratio) = saved.minimum_cruise_ratio {
        limit.push_str(&format!(" MINIMUM_CRUISE_RATIO={}", num(ratio)));
    }
    let mut lines = vec![limit];
    if let Some(s) = &saved.shaper {
        lines.push(format!(
            "SET_INPUT_SHAPER SHAPER_TYPE_X={} SHAPER_FREQ_X={} DAMPING_RATIO_X={} \
             SHAPER_TYPE_Y={} SHAPER_FREQ_Y={} DAMPING_RATIO_Y={}",
            s.type_x,
            num(s.freq_x),
            num(s.damping_x),
            s.type_y,
            num(s.freq_y),
            num(s.damping_y)
        ));
    }
    lines.push("G90".to_owned());
    lines.push(format!("RESTORE_GCODE_STATE NAME={STATE_NAME}"));
    lines.join("\n")
}

/// How putting the settings back went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Restore {
    /// Everything was put back.
    Done,
    /// Nothing needed putting back, because Klipper shut down and its defaults apply on restart.
    NotNeeded,
    /// The restore failed.
    Failed {
        /// What the hold changed and may still be changed.
        left_changed: Vec<String>,
        /// The G-code that puts it back, to run by hand.
        fix: String,
    },
}

impl Restore {
    /// A failed restore of `saved`: what is left changed and the G-code that fixes it.
    #[must_use]
    pub fn failed(saved: &Saved, reason: &str) -> Self {
        let mut left = vec![format!(
            "max_accel {} (the hold raised it; restore failed: {reason})",
            num(saved.max_accel)
        )];
        if let Some(ratio) = saved.minimum_cruise_ratio {
            left.push(format!(
                "minimum_cruise_ratio {} (the hold set it to 0)",
                num(ratio)
            ));
        }
        if saved.shaper.is_some() {
            left.push("input shaping (the hold turned it off)".to_owned());
        }
        left.push("positioning mode (the hold set relative)".to_owned());
        Self::Failed {
            left_changed: left,
            fix: restore_script(saved),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::hold::pieces::{Drive, HoldAxis};

    fn shaper() -> Shaper {
        Shaper {
            type_x: "mzv".to_owned(),
            freq_x: 51.2,
            damping_x: 0.1,
            type_y: "mzv".to_owned(),
            freq_y: 44.0,
            damping_y: 0.1,
        }
    }

    fn saved() -> Saved {
        Saved {
            max_velocity: 500.0,
            max_accel: 3000.0,
            minimum_cruise_ratio: Some(0.5),
            shaper: Some(shaper()),
        }
    }

    fn drive(frequency: f64, accel_per_hz: f64) -> Drive {
        Drive {
            axis: HoldAxis::X,
            frequency,
            accel_per_hz,
        }
    }

    #[test]
    fn setup_raises_the_limit_and_turns_shaping_off() {
        let script = setup_script(&saved(), &drive(150.0, 75.0));
        assert_eq!(
            script.lines().collect::<Vec<_>>(),
            [
                "SET_VELOCITY_LIMIT ACCEL=11250 MINIMUM_CRUISE_RATIO=0",
                "SET_INPUT_SHAPER SHAPER_FREQ_X=0 SHAPER_FREQ_Y=0"
            ]
        );
        let bare = Saved {
            minimum_cruise_ratio: None,
            shaper: None,
            ..saved()
        };
        assert_eq!(
            setup_script(&bare, &drive(150.0, 75.0)),
            "SET_VELOCITY_LIMIT ACCEL=11250"
        );
    }

    #[test]
    fn accel_line_follows_the_frequency() {
        assert_eq!(
            accel_line(&drive(100.0, 60.0)),
            "SET_VELOCITY_LIMIT ACCEL=6000"
        );
    }

    #[test]
    fn restore_puts_everything_back() {
        assert_eq!(
            restore_script(&saved()).lines().collect::<Vec<_>>(),
            [
                "SET_VELOCITY_LIMIT VELOCITY=500 ACCEL=3000 MINIMUM_CRUISE_RATIO=0.5",
                "SET_INPUT_SHAPER SHAPER_TYPE_X=mzv SHAPER_FREQ_X=51.2 DAMPING_RATIO_X=0.1 \
                 SHAPER_TYPE_Y=mzv SHAPER_FREQ_Y=44 DAMPING_RATIO_Y=0.1",
                "G90",
                "RESTORE_GCODE_STATE NAME=wobblescope_hold"
            ]
        );
    }

    #[test]
    fn from_status_reads_the_printers_objects() {
        let status = json!({
            "toolhead": {"max_velocity": 500.0, "max_accel": 3000.0, "minimum_cruise_ratio": 0.5},
            "input_shaper": {
                "shaper_type_x": "mzv", "shaper_freq_x": 51.2, "damping_ratio_x": 0.1,
                "shaper_type_y": "mzv", "shaper_freq_y": 44.0, "damping_ratio_y": 0.1
            }
        });
        assert_eq!(Saved::from_status(&status).unwrap(), saved());
        let without = json!({"toolhead": {"max_velocity": 500.0, "max_accel": 3000.0}});
        let got = Saved::from_status(&without).unwrap();
        assert_eq!(got.shaper, None);
        assert_eq!(got.minimum_cruise_ratio, None);
        let empty_shaper = json!({
            "toolhead": {"max_velocity": 500.0, "max_accel": 3000.0},
            "input_shaper": {}
        });
        assert_eq!(Saved::from_status(&empty_shaper).unwrap().shaper, None);
        let missing = json!({"toolhead": {"max_velocity": 500.0}});
        assert!(
            Saved::from_status(&missing)
                .unwrap_err()
                .contains("max_accel")
        );
    }

    #[test]
    fn a_failed_restore_names_what_is_left() {
        let Restore::Failed { left_changed, fix } = Restore::failed(&saved(), "boom") else {
            panic!("expected a failed restore");
        };
        let all = left_changed.join("\n");
        assert!(all.contains("max_accel 3000"), "{all}");
        assert!(all.contains("input shaping"), "{all}");
        assert!(all.contains("positioning mode"), "{all}");
        assert_eq!(fix, restore_script(&saved()));
    }
}
