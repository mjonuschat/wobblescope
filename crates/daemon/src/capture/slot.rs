//! The one-test-at-a-time slot, the quarantine clock and the motion-uncertain marker.
//!
//! The slot is a permit like [`crate::api::ImportGate`]. Emergency stop does not go through the
//! runner: it reads the current test from here, sets the stop flag and calls Moonraker itself.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::lock::lock;

#[derive(Default)]
struct State {
    /// The id of the test that holds the slot, once it has one.
    id: Option<String>,
    held: bool,
    stop: Arc<AtomicBool>,
    quarantine_until: Option<Instant>,
    /// A finished test whose last command may still be moving the toolhead.
    uncertain: Option<String>,
}

/// The slot. Clones share it.
#[derive(Clone, Default)]
pub struct TestSlot {
    state: Arc<Mutex<State>>,
}

/// The right to run a test. Dropping it frees the slot.
pub struct SlotPermit {
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
}

impl SlotPermit {
    /// Names the test that holds the slot, so a stop can find it.
    pub fn begin(&self, id: &str) {
        lock(&self.state).id = Some(id.to_owned());
    }

    /// Whether an emergency stop was requested for this test.
    #[must_use]
    pub fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    /// A handle to the stop flag, for tasks that wait on it.
    #[must_use]
    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }
}

impl Drop for SlotPermit {
    fn drop(&mut self) {
        let mut s = lock(&self.state);
        s.held = false;
        s.id = None;
    }
}

impl TestSlot {
    /// Takes the slot, or `None` when a test holds it.
    #[must_use]
    pub fn try_acquire(&self) -> Option<SlotPermit> {
        let mut s = lock(&self.state);
        if s.held {
            return None;
        }
        s.held = true;
        s.id = None;
        s.stop = Arc::new(AtomicBool::new(false));
        Some(SlotPermit {
            state: self.state.clone(),
            stop: s.stop.clone(),
        })
    }

    /// Whether a test holds the slot.
    #[must_use]
    pub fn is_held(&self) -> bool {
        lock(&self.state).held
    }

    /// The id of the test that holds the slot, once it has named itself.
    #[must_use]
    pub fn current_id(&self) -> Option<String> {
        let s = lock(&self.state);
        s.id.clone().filter(|_| s.held)
    }

    /// Asks the running test `id` to stop. Returns whether such a test holds the slot.
    #[must_use]
    pub fn request_stop(&self, id: &str) -> bool {
        let s = lock(&self.state);
        if s.held && s.id.as_deref() == Some(id) {
            s.stop.store(true, Ordering::Release);
            true
        } else {
            false
        }
    }

    #[must_use]
    pub fn request_stop_current(&self) -> Option<String> {
        let s = lock(&self.state);
        let id = s.id.clone().filter(|_| s.held)?;
        s.stop.store(true, Ordering::Release);
        Some(id)
    }

    /// Withdraws a stop request, for a stop whose dispatch to Moonraker failed.
    pub fn cancel_stop(&self, id: &str) {
        let s = lock(&self.state);
        if s.id.as_deref() == Some(id) {
            s.stop.store(false, Ordering::Release);
        }
    }

    /// Starts the quarantine: no test may start for `duration`.
    pub fn start_quarantine(&self, duration: Duration) {
        let until = Instant::now() + duration;
        let mut s = lock(&self.state);
        if s.quarantine_until.is_none_or(|u| u < until) {
            s.quarantine_until = Some(until);
        }
    }

    /// The time left in the quarantine, or `None` when there is none.
    #[must_use]
    pub fn quarantine_remaining(&self) -> Option<Duration> {
        let s = lock(&self.state);
        s.quarantine_until
            .and_then(|u| u.checked_duration_since(Instant::now()))
            .filter(|d| !d.is_zero())
    }

    /// Marks the finished test `id` as one whose motion may be uncertain.
    pub fn set_motion_uncertain(&self, id: &str) {
        lock(&self.state).uncertain = Some(id.to_owned());
    }

    /// Clears the marker.
    pub fn clear_motion_uncertain(&self) {
        lock(&self.state).uncertain = None;
    }

    /// The finished test whose motion may be uncertain.
    #[must_use]
    pub fn motion_uncertain(&self) -> Option<String> {
        lock(&self.state).uncertain.clone()
    }
}
