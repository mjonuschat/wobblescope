//! What the live socket shows: the running test's state and the last finished test. A hub holds
//! the current snapshot and broadcasts every change, so a new subscriber starts from the
//! snapshot and a slow one is given a fresh snapshot instead of a closed socket.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;
use wobblescope_core::fit::Axis;

use crate::lock::lock;

/// How many changes a subscriber may lag behind before it is given a fresh snapshot.
const BROADCAST_CAPACITY: usize = 256;

/// Where a running test is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Phase {
    /// Moving the toolhead to the segment's point.
    Positioning,
    /// The sweep is running.
    Measuring,
    /// Waiting for every chip's data to pass the window end.
    Draining,
    /// A failure is recorded and the daemon waits for the running sweep to end.
    WaitingForSweep,
    /// Writing the session.
    Sealing,
}

/// What one chip shows during a test.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ChipLive {
    /// The chip's config section.
    pub chip: String,
    /// RMS signal per axis (x, y, z) in mm/s², after removing the mean of the latest batch.
    pub level: Option<[f64; 3]>,
    /// Health warnings seen so far in this test.
    pub warnings: Vec<String>,
}

/// A running test.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TestState {
    /// The test id, which is also the session id.
    pub id: String,
    /// Where the test is.
    pub phase: Phase,
    /// The current segment, counting from 1.
    pub segment: u32,
    /// How many segments the test has.
    pub segments: u32,
    /// The axis of the current segment.
    pub axis: Option<Axis>,
    /// The toolhead position of the current segment: `[x, y, z]` in mm.
    pub position: Option<[f64; 3]>,
    /// How long a segment is expected to take, in seconds.
    pub expected_segment_seconds: f64,
    /// How long the current segment has run, in seconds.
    pub elapsed_seconds: f64,
    /// The frequency Klipper last reported testing, in Hz, within the current segment. It is the
    /// whole-number frequency of "Testing frequency N Hz" and runs slightly ahead of the motion,
    /// because Klipper reports it when it queues the move.
    pub frequency_hz: Option<f64>,
    /// The lowest and highest frequency of every segment, in Hz, so a chart can fix its axis.
    pub frequency_range: [f64; 2],
    /// Per-chip level and warnings.
    pub chips: Vec<ChipLive>,
}

/// How a finished test ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum FinishedOutcome {
    /// Every segment finished and every chip delivered.
    Complete,
    /// A chip stopped delivering; the data it did deliver is kept.
    Incomplete,
    /// The test failed; the finished segments are kept.
    Failed,
    /// The test failed before the first window began, so no session exists.
    NoSession,
}

/// A test that has ended.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FinishedTest {
    /// The test id.
    pub id: String,
    /// How it ended.
    pub outcome: FinishedOutcome,
    /// Why, for anything but a complete test.
    pub reason: Option<String>,
    /// The session, when one exists.
    pub session_id: Option<String>,
    /// Whether the toolhead may still be moving, so an emergency stop is still accepted.
    pub motion_uncertain: bool,
}

/// The live socket's first message, and every change after it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct LiveSnapshot {
    /// The running test, if any.
    pub test: Option<TestState>,
    /// The last test that ended.
    pub last: Option<FinishedTest>,
}

/// The hub. Clones share it.
#[derive(Clone)]
pub struct LiveHub {
    current: Arc<Mutex<LiveSnapshot>>,
    changes: broadcast::Sender<LiveSnapshot>,
}

impl Default for LiveHub {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveHub {
    /// An empty hub: no test, no last test.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: Arc::default(),
            changes: broadcast::channel(BROADCAST_CAPACITY).0,
        }
    }

    /// The current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> LiveSnapshot {
        lock(&self.current).clone()
    }

    /// The current snapshot and a receiver of every change after it.
    #[must_use]
    pub fn subscribe(&self) -> (LiveSnapshot, broadcast::Receiver<LiveSnapshot>) {
        let current = lock(&self.current);
        let rx = self.changes.subscribe();
        (current.clone(), rx)
    }

    /// Changes the snapshot with `f` and broadcasts the result when it changed.
    pub fn update(&self, f: impl FnOnce(&mut LiveSnapshot)) {
        let mut current = lock(&self.current);
        let before = current.clone();
        f(&mut current);
        if *current != before {
            let _ = self.changes.send(current.clone());
        }
    }
}
