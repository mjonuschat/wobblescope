//! Capture: running a resonance test and recording what the accelerometers see.

pub mod availability;
pub mod axes_map;
mod calibration;
pub mod live;
pub mod meta;
pub mod position;
pub mod preflight;
pub mod recorder;
pub mod request;
pub mod runner;
mod seal;
pub mod session_id;
pub mod slot;
pub mod vibrations;

use serde::Serialize;
use time::{OffsetDateTime, UtcOffset};

/// `now` as `YYYY-MM-DDTHH:MM:SSZ`.
#[must_use]
pub fn iso_utc(now: OffsetDateTime) -> String {
    let now = now.to_offset(UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// The stable name of a pre-flight check. It goes over the wire in snake case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum CheckId {
    NotPrinting,
    Homed,
    MoonrakerConnected,
    KlipperReady,
    Quarantine,
    NoTestRunning,
    StorageOpen,
    DiskSpace,
    NoUncertainMotion,
    ToolheadIdle,
    Request,
    ChipsAvailable,
}

/// One pre-flight check that failed: a stable name and a message for a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CheckFailure {
    /// The check that failed.
    pub check: CheckId,
    /// What is wrong and how to fix it.
    pub message: String,
}

impl CheckFailure {
    /// A failure of `check` with `message`.
    #[must_use]
    pub fn new(check: CheckId, message: impl Into<String>) -> Self {
        Self {
            check,
            message: message.into(),
        }
    }
}
