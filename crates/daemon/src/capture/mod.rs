//! Capture: running a resonance test and recording what the accelerometers see.

pub mod availability;

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

/// One pre-flight check that failed: a stable name and a message for a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CheckFailure {
    /// The check's stable name, for example `klipper_ready`.
    pub check: String,
    /// What is wrong and how to fix it.
    pub message: String,
}

impl CheckFailure {
    /// A failure of `check` with `message`.
    #[must_use]
    pub fn new(check: &str, message: impl Into<String>) -> Self {
        Self {
            check: check.to_owned(),
            message: message.into(),
        }
    }
}
