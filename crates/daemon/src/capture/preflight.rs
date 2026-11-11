//! Pre-flight: deciding whether a test may start, and saying why not, completely.
//!
//! Every check but the last two runs and the failures come back together. Only when those pass
//! does the chip check open the capture connections, and only when they pass does the disk check
//! reserve space. A refusal sends no motion command.

use std::time::Duration;

use serde_json::{Value, json};
use time::OffsetDateTime;

use super::CheckFailure;
use super::availability::{AVAILABILITY_TIMEOUT, OpenChip, open_chips};
use super::request::TestPlan;
use super::session_id::{base_id, first_free_id};
use super::slot::TestSlot;
use crate::capture::iso_utc;
use crate::moonraker::Moonraker;
use crate::moonraker::rpc::{Rpc, RpcError};
use crate::moonraker::status::{KlippyState, MoonrakerState};
use crate::startup::StorageCell;
use crate::storage::estimate::estimate_bytes;
use crate::storage::{Reservation, StorageError};

/// How long the printer-state query may take.
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);

/// The margin added to `max_segment_seconds` for the quarantine, in seconds.
pub const QUARANTINE_MARGIN_SECONDS: u64 = 60;

/// How long the quarantine lasts for the daemon's longest possible command.
#[must_use]
pub fn quarantine_duration(max_segment_seconds: u32) -> Duration {
    Duration::from_secs(u64::from(max_segment_seconds) + QUARANTINE_MARGIN_SECONDS)
}

/// What the printer reports about itself, as pre-flight needs it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrinterState {
    /// `print_stats.state`.
    pub print_state: Option<String>,
    /// `idle_timeout.state`.
    pub idle_state: Option<String>,
    /// `toolhead.homed_axes`.
    pub homed_axes: Option<String>,
}

/// Reads [`PrinterState`] from Moonraker.
///
/// # Errors
/// The call's error.
pub async fn read_printer_state(control: &Rpc) -> Result<PrinterState, RpcError> {
    let query = json!({"objects": {
        "print_stats": ["state"],
        "idle_timeout": ["state"],
        "toolhead": ["homed_axes"],
    }});
    let answer = control
        .call("printer.objects.query", Some(query), QUERY_TIMEOUT)
        .await?;
    let field = |object: &str, key: &str| {
        answer
            .get("status")
            .and_then(|s| s.get(object))
            .and_then(|o| o.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    Ok(PrinterState {
        print_state: field("print_stats", "state"),
        idle_state: field("idle_timeout", "state"),
        homed_axes: field("toolhead", "homed_axes"),
    })
}

impl PrinterState {
    /// Whether the toolhead is idle. Without an `idle_timeout` state idleness cannot be shown,
    /// so it counts as idle outside quarantine and as not idle inside it.
    #[must_use]
    pub fn idle(&self, quarantined: bool) -> bool {
        match self.idle_state.as_deref() {
            Some(state) => state != "Printing",
            None => !quarantined,
        }
    }

    /// The failed printer-state checks: `not_printing`, `toolhead_idle` and `homed`.
    #[must_use]
    pub fn failures(&self, quarantined: bool) -> Vec<CheckFailure> {
        let mut out = Vec::new();
        if let Some(state @ ("printing" | "paused")) = self.print_state.as_deref() {
            out.push(CheckFailure::new(
                "not_printing",
                format!("a print is {state}; wait for it to end or cancel it"),
            ));
        }
        if !self.idle(quarantined) {
            let why = match self.idle_state.as_deref() {
                Some(state) => format!("the toolhead is {state}; wait until it is idle"),
                None => "the toolhead's idle state is not reported, so it cannot be shown to be \
                         idle while a sweep may still be running"
                    .to_owned(),
            };
            out.push(CheckFailure::new("toolhead_idle", why));
        }
        let homed = self.homed_axes.as_deref().unwrap_or_default();
        let missing: Vec<char> = ['x', 'y', 'z']
            .into_iter()
            .filter(|a| !homed.contains(*a))
            .collect();
        if !missing.is_empty() {
            let names: Vec<String> = missing
                .iter()
                .map(|a| a.to_ascii_uppercase().to_string())
                .collect();
            out.push(CheckFailure::new(
                "homed",
                format!("home X, Y and Z first; not homed: {}", names.join(", ")),
            ));
        }
        out
    }
}

/// The connection and Klipper-state checks, then the printer-state checks.
pub async fn check_printer(moonraker: &Moonraker, quarantined: bool) -> Vec<CheckFailure> {
    let status = moonraker.status();
    let mut out = Vec::new();
    let Some(control) = moonraker
        .control()
        .filter(|_| status.moonraker.state == MoonrakerState::Connected)
    else {
        out.push(CheckFailure::new(
            "moonraker_connected",
            "the daemon is not connected to Moonraker",
        ));
        return out;
    };
    let ready = status.klippy.state == KlippyState::Ready;
    if !ready {
        out.push(CheckFailure::new(
            "klipper_ready",
            format!("Klipper is {}, not ready", status.klippy.state.as_str()),
        ));
    }
    // The state is read even when Klipper is not ready, so one answer lists everything that is
    // wrong. A failed read then adds nothing: Klipper's state already explains it.
    match read_printer_state(&control).await {
        Ok(state) => out.extend(state.failures(quarantined)),
        Err(e) if ready => out.push(CheckFailure::new(
            "moonraker_connected",
            format!("cannot read the printer's state: {e}"),
        )),
        Err(_) => {}
    }
    out
}

/// Why a test may not start.
#[derive(Debug)]
pub enum Refusal {
    /// One or more checks failed.
    Checks(Vec<CheckFailure>),
    /// The disk or the retention cap cannot hold the test.
    Storage(StorageError),
}

/// A test that passed pre-flight: the proven connections and the reserved space.
pub struct Started {
    /// The session id, which is also the test id and Klipper's `NAME`.
    pub id: String,
    /// When the test was admitted, as an ISO 8601 UTC time.
    pub created_at: String,
    /// The proven chip connections, in the order of the request's chips.
    pub chips: Vec<OpenChip>,
    /// The space held for the session.
    pub reservation: Reservation,
}

/// What pre-flight needs.
pub struct Deps<'a> {
    /// The Moonraker handle.
    pub moonraker: &'a Moonraker,
    /// The storage, once it is open.
    pub storage: &'a StorageCell,
    /// The slot, for the quarantine.
    pub slot: &'a TestSlot,
    /// How long chips may take to prove themselves.
    pub availability_timeout: Duration,
}

impl<'a> Deps<'a> {
    /// The usual timeout.
    #[must_use]
    pub fn new(moonraker: &'a Moonraker, storage: &'a StorageCell, slot: &'a TestSlot) -> Self {
        Self {
            moonraker,
            storage,
            slot,
            availability_timeout: AVAILABILITY_TIMEOUT,
        }
    }
}

/// The failed check for a quarantine with `left` still to run.
#[must_use]
pub fn quarantine_failure(left: Duration) -> CheckFailure {
    CheckFailure::new(
        "quarantine",
        format!(
            "a sweep may still be running after a restart or a lost connection; no test can \
             start for another {} s",
            left.as_secs() + 1
        ),
    )
}

/// Runs every check for `plan`. `busy` says another test already holds the slot.
///
/// # Errors
/// A [`Refusal`]: every failed check at once, or the storage error that refused the space.
pub async fn run(
    deps: &Deps<'_>,
    plan: &TestPlan,
    busy: bool,
    now: OffsetDateTime,
) -> Result<Started, Refusal> {
    let quarantine = deps.slot.quarantine_remaining();
    let mut failures = check_printer(deps.moonraker, quarantine.is_some()).await;
    if busy {
        failures.push(CheckFailure::new(
            "no_test_running",
            "another test is already running",
        ));
    }
    if let Some(left) = quarantine {
        failures.push(quarantine_failure(left));
    }
    let Some(storage) = deps.storage.get() else {
        failures.push(CheckFailure::new(
            "storage_open",
            "the daemon is waiting for Moonraker to report where to store sessions",
        ));
        return Err(Refusal::Checks(failures));
    };
    if !failures.is_empty() {
        return Err(Refusal::Checks(failures));
    }

    let snapshot = deps.moonraker.chips();
    let mut unknown = Vec::new();
    let wanted: Vec<_> = plan
        .chips
        .iter()
        .filter_map(|name| {
            let found = snapshot.list.chips.iter().find(|c| c.section == *name);
            if found.is_none() {
                unknown.push(format!("{name} is not a chip in Klipper's config"));
            }
            found.cloned()
        })
        .collect();
    if !unknown.is_empty() {
        return Err(chips_refused(unknown));
    }
    let outcomes = open_chips(deps.moonraker, &wanted, deps.availability_timeout)
        .await
        .map_err(|e| {
            Refusal::Checks(vec![CheckFailure::new(
                "moonraker_connected",
                e.to_string(),
            )])
        })?;
    let mut chips = Vec::new();
    let mut bad = Vec::new();
    for (chip, outcome) in outcomes {
        match outcome {
            Ok(open) => chips.push(open),
            Err(reason) => bad.push(format!("{} is not available: {reason}", chip.section)),
        }
    }
    if !bad.is_empty() {
        // Dropping `chips` closes the connections that did open.
        return Err(chips_refused(bad));
    }

    let recordings: Vec<(f64, f64)> = chips
        .iter()
        .flat_map(|c| {
            plan.segments
                .iter()
                .map(move |_| (c.observed_rate_hz, plan.segment_seconds))
        })
        .collect();
    let estimate = estimate_bytes(&recordings).unwrap_or(u64::MAX);
    let created_at = iso_utc(now);
    let base = base_id("test", &created_at);
    let admitted =
        first_free_id(&base, |id| storage.admit(id, estimate)).map_err(Refusal::Storage)?;
    let Some((id, reservation)) = admitted else {
        return Err(Refusal::Checks(vec![CheckFailure::new(
            "disk_space",
            "no free test id for this second",
        )]));
    };
    Ok(Started {
        id,
        created_at,
        chips,
        reservation,
    })
}

fn chips_refused(reasons: Vec<String>) -> Refusal {
    Refusal::Checks(
        reasons
            .into_iter()
            .map(|r| CheckFailure::new("chips_available", r))
            .collect(),
    )
}
