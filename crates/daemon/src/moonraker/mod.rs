//! The daemon's connection to Moonraker and what it learns from it.

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::{broadcast, mpsc, oneshot, watch};

use crate::access;
use crate::access::AccessState;
use crate::config::Settings;
use rpc::Rpc;

pub mod bridge;
pub mod chips;
pub mod discovery;
mod link;
pub mod rpc;
pub mod settings;
pub mod status;

use chips::Availability;
use discovery::{ChipList, RootsError};
use status::Status;

/// The chips the printer's config lists, and how fresh that list is.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChipsSnapshot {
    /// Whether a list has been read since the daemon started.
    pub known: bool,
    /// Whether the list comes from Klipper's current ready period.
    pub current: bool,
    /// The last list read.
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub list: ChipList,
}

/// The supervisor's timings. Tests set them to milliseconds.
#[derive(Debug, Clone, Copy)]
pub struct Timings {
    /// How long opening the websocket may take.
    pub connect: Duration,
    /// How long an ordinary request may take.
    pub request: Duration,
    /// How long the config query may take; it can be large.
    pub configfile: Duration,
    /// The poll interval while Klipper is ready.
    pub poll_ready: Duration,
    /// The poll interval while Klipper is not ready.
    pub poll_not_ready: Duration,
    /// The first wait after a failed connection.
    pub backoff_start: Duration,
    /// The longest wait between attempts.
    pub backoff_max: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            request: Duration::from_secs(10),
            configfile: Duration::from_secs(30),
            poll_ready: Duration::from_secs(10),
            poll_not_ready: Duration::from_secs(2),
            backoff_start: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
        }
    }
}

/// A read-only view of what the supervisor publishes. Clones share it.
#[derive(Clone)]
pub struct Moonraker {
    status: watch::Receiver<Status>,
    chips: watch::Receiver<ChipsSnapshot>,
    chips_writer: std::sync::Arc<watch::Sender<ChipsSnapshot>>,
    control: watch::Receiver<Option<Rpc>>,
    access: watch::Receiver<AccessState>,
    gcode: broadcast::Sender<String>,
    bridge_base: std::sync::Arc<str>,
}

impl ChipsSnapshot {
    /// Forgets every availability result and reports whether there was one.
    pub fn reset_availability(&mut self) -> bool {
        let mut had = false;
        for chip in &mut self.list.chips {
            had |= chip.availability != Availability::Unchecked;
            chip.availability = Availability::Unchecked;
        }
        had
    }
}

impl Moonraker {
    /// The latest status.
    #[must_use]
    pub fn status(&self) -> Status {
        self.status.borrow().clone()
    }

    /// The latest chip list.
    #[must_use]
    pub fn chips(&self) -> ChipsSnapshot {
        self.chips.borrow().clone()
    }

    /// Records what a check found. A result for a section that is no longer listed is dropped,
    /// and so is every result when the list is not current: a restart overtook the check.
    pub fn set_availability(&self, results: Vec<(String, Availability)>) {
        self.chips_writer.send_if_modified(|snapshot| {
            if !snapshot.current {
                return false;
            }
            let mut changed = false;
            for (section, availability) in results {
                if let Some(chip) = snapshot
                    .list
                    .chips
                    .iter_mut()
                    .find(|c| c.section == section)
                {
                    chip.availability = availability;
                    changed = true;
                }
            }
            changed
        });
    }

    /// The current control connection, or `None` while Moonraker is not connected.
    #[must_use]
    pub fn control(&self) -> Option<Rpc> {
        self.control.borrow().clone()
    }

    /// The lines Klipper writes to the console, as Moonraker reports them. A receiver sees only
    /// the lines sent after it subscribed.
    #[must_use]
    pub fn gcode_lines(&self) -> broadcast::Receiver<String> {
        self.gcode.subscribe()
    }

    /// What Moonraker's lists currently say about who may use the daemon.
    #[must_use]
    pub fn access(&self) -> AccessState {
        self.access.borrow().clone()
    }

    /// A receiver that changes whenever the access state does.
    #[must_use]
    pub fn watch_access(&self) -> watch::Receiver<AccessState> {
        self.access.clone()
    }

    /// A receiver that changes whenever Moonraker's or Klipper's state does.
    #[must_use]
    pub fn watch_status(&self) -> watch::Receiver<Status> {
        self.status.clone()
    }

    /// A receiver that changes whenever the control connection appears or goes away.
    #[must_use]
    pub fn watch_control(&self) -> watch::Receiver<Option<Rpc>> {
        self.control.clone()
    }

    /// The websocket URL of Moonraker's bridge to Klipper's own socket, with the one-shot
    /// `token` in the query.
    #[must_use]
    pub fn bridge_url(&self, token: &str) -> String {
        format!("{}?token={}", self.bridge_base, encode_query(token))
    }
}

/// Percent-encodes everything but unreserved characters.
fn encode_query(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The writing half of the pair: what the supervisor publishes through.
pub struct Publisher {
    status: watch::Sender<Status>,
    chips: std::sync::Arc<watch::Sender<ChipsSnapshot>>,
    control: watch::Sender<Option<Rpc>>,
    access: std::sync::Arc<watch::Sender<AccessState>>,
    gcode: broadcast::Sender<String>,
}

impl Publisher {
    /// Publishes a line Klipper wrote to the console.
    pub fn publish_gcode(&self, line: String) {
        // No receiver is the normal case between tests.
        let _ = self.gcode.send(line);
    }

    /// Publishes a status.
    pub fn set_status(&self, status: Status) {
        self.status.send_replace(status);
    }

    /// Publishes a chip list.
    pub fn set_chips(&self, chips: ChipsSnapshot) {
        self.chips.send_replace(chips);
    }

    /// Publishes the control connection, or `None` when there is none.
    ///
    /// Losing the connection also turns the access state unknown, at once and keeping the last
    /// login requirement, so no policy read on it stays in force.
    pub fn set_control(&self, control: Option<Rpc>) {
        let lost = control.is_none();
        // First, so that a publish checking the connection under the access lock sees the change.
        self.control.send_replace(control);
        if lost {
            self.access.send_if_modified(|current| {
                let logins_required = match current {
                    AccessState::Known(policy) => policy.logins_required(),
                    AccessState::Unknown { logins_required } => *logins_required,
                };
                let unknown = AccessState::Unknown { logins_required };
                let changed = *current != unknown;
                if changed {
                    *current = unknown;
                }
                changed
            });
        }
    }

    /// Publishes the access state, waking readers only when it differs from the current one.
    pub fn set_access(&self, access: AccessState) {
        publish_access(&self.access, access);
    }
}

/// Publishes `access` through `sender`, waking readers only when it differs from the current one.
pub(crate) fn publish_access(sender: &watch::Sender<AccessState>, access: AccessState) {
    publish_access_if(sender, access, || true);
}

/// Like [`publish_access`], but publishes nothing unless `still_current` says so. The check runs
/// while the sender is locked, so a caller that checks its connection there cannot publish after
/// [`Publisher::set_control`] has dropped it. That call replaces the connection first and
/// takes this lock second.
pub(crate) fn publish_access_if(
    sender: &watch::Sender<AccessState>,
    access: AccessState,
    still_current: impl FnOnce() -> bool,
) {
    sender.send_if_modified(|current| {
        if !still_current() {
            return false;
        }
        let changed = *current != access;
        if changed {
            *current = access;
        }
        changed
    });
}

/// How many console lines a slow receiver may fall behind before it loses the oldest.
const GCODE_LINES: usize = 256;

/// A publisher and the handle that reads it, starting from "connecting" and no chips.
#[must_use]
pub fn channel() -> (Publisher, Moonraker) {
    let (status_tx, status) = watch::channel(Status::default());
    let (chips_tx, chips) = watch::channel(ChipsSnapshot::default());
    let chips_tx = std::sync::Arc::new(chips_tx);
    let (control_tx, control) = watch::channel(None);
    let (access_tx, access) = watch::channel(AccessState::default());
    let access_tx = std::sync::Arc::new(access_tx);
    let (gcode_tx, _) = broadcast::channel(GCODE_LINES);
    (
        Publisher {
            status: status_tx,
            chips: chips_tx.clone(),
            control: control_tx,
            access: access_tx,
            gcode: gcode_tx.clone(),
        },
        Moonraker {
            status,
            chips,
            chips_writer: chips_tx,
            control,
            access,
            gcode: gcode_tx,
            bridge_base: Settings::default().moonraker_bridge_base().into(),
        },
    )
}

/// The answer to "where is the config root": the path, or why there is none.
pub type ConfigRootReport = oneshot::Receiver<Result<PathBuf, RootsError>>;

/// Starts the supervisor for the Moonraker named in `settings`.
///
/// With `want_config_root` the supervisor asks Moonraker for its file roots once, on the first
/// successful connection, and reports the config root through the returned receiver. The
/// supervisor stops when every returned handle is dropped.
///
/// Must be called inside a tokio runtime.
#[must_use]
pub fn spawn(
    settings: &Settings,
    timings: Timings,
    want_config_root: bool,
) -> (Moonraker, Option<ConfigRootReport>) {
    let (publisher, mut handle) = channel();
    let access_sender = publisher.access.clone();
    let (refused_tx, refused) = mpsc::unbounded_channel();
    handle.bridge_base = settings.moonraker_bridge_base().into();
    let (report, receiver) = if want_config_root {
        let (tx, rx) = oneshot::channel();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };
    let link = link::Link {
        ws_url: settings.moonraker_ws_url(),
        base_url: settings
            .moonraker_url
            .to_string()
            .trim_end_matches('/')
            .to_owned(),
        api_key: settings.moonraker_api_key.clone(),
        timings,
        publisher,
        root_report: report,
        refused,
    };
    tokio::spawn(access::watcher::run(
        handle.control.clone(),
        access_sender,
        timings,
        refused_tx,
    ));
    tokio::spawn(link.run());
    (handle, receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_publish_whose_guard_fails_changes_nothing() {
        let (tx, rx) = watch::channel(AccessState::default());
        let known = AccessState::Unknown {
            logins_required: true,
        };
        publish_access_if(&tx, known.clone(), || false);
        assert_eq!(*rx.borrow(), AccessState::default());
        publish_access_if(&tx, known.clone(), || true);
        assert_eq!(*rx.borrow(), known);
    }
}
