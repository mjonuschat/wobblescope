//! The daemon's connection to Moonraker and what it learns from it.

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::{oneshot, watch};

use crate::config::Settings;

pub mod chips;
pub mod discovery;
mod link;
pub mod rpc;
pub mod status;

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
}

/// The writing half of the pair: what the supervisor publishes through.
pub struct Publisher {
    status: watch::Sender<Status>,
    chips: watch::Sender<ChipsSnapshot>,
}

impl Publisher {
    /// Publishes a status.
    pub fn set_status(&self, status: Status) {
        self.status.send_replace(status);
    }

    /// Publishes a chip list.
    pub fn set_chips(&self, chips: ChipsSnapshot) {
        self.chips.send_replace(chips);
    }
}

/// A publisher and the handle that reads it, starting from "connecting" and no chips.
#[must_use]
pub fn channel() -> (Publisher, Moonraker) {
    let (status_tx, status) = watch::channel(Status::default());
    let (chips_tx, chips) = watch::channel(ChipsSnapshot::default());
    (
        Publisher {
            status: status_tx,
            chips: chips_tx,
        },
        Moonraker { status, chips },
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
    let (publisher, handle) = channel();
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
    };
    tokio::spawn(link.run());
    (handle, receiver)
}
