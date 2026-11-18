use std::time::Instant;

use wobblescope_core::eligibility::Coverage;
use wobblescope_session::meta::TimeWindow;

use super::{Failure, Run, SegmentOutcome, Stored, watch_idle};
use crate::capture::live::{FinishedTest, Phase};
use crate::capture::preflight::quarantine_duration;
use crate::capture::recorder::Recorder;
use crate::capture::seal::{Sealing, abandon, seal};
use crate::moonraker::status::KlippyState;

impl Run {
    /// Adds the segment that was running when the test failed, as incomplete, when at least one
    /// chip delivered a sample at or after its window start.
    pub(super) async fn add_interrupted_segment(&mut self) {
        if self.plan.vibrations.is_some() {
            self.add_interrupted_angle().await;
            return;
        }
        let Some(current) = &self.current else { return };
        let Some(start) = current.start else { return };
        let index = current.index;
        let end = self
            .recorders
            .iter()
            .filter_map(Recorder::last_timestamp)
            .fold(f64::NEG_INFINITY, f64::max);
        if !end.is_finite() || end <= start {
            return;
        }
        let mut outcome = SegmentOutcome::default();
        for (chip, rec) in self.recorders.iter().enumerate() {
            let taken = rec.take(start, end);
            if taken.samples.t.iter().any(|t| *t >= start) {
                let coverage = Coverage::Incomplete {
                    missing: "the test was interrupted".to_owned(),
                };
                outcome.add(chip, taken, coverage);
            }
        }
        if outcome.recordings.is_empty() {
            return;
        }
        let segment = self.plan.segments[index];
        let at = self.segments_stored();
        self.incomplete
            .push(format!("segment {} was interrupted", at + 1));
        // A failure to write it leaves the finished segments, which are already journaled.
        let _ = self
            .store_segment(&segment, at, outcome, TimeWindow { start, end }, vec![])
            .await;
    }

    /// Klipper's state changes before its message arrives. Waits a little for the message so the
    /// session says why Klipper stopped.
    pub(super) async fn refresh_klipper_message(&mut self, failure: &mut Failure) {
        let deadline = Instant::now() + self.r.timings.shutdown_grace;
        loop {
            if let Some(message) = self.status.borrow().klippy.message.clone() {
                failure.reason = message;
                return;
            }
            if Instant::now() >= deadline {
                return;
            }
            tokio::select! {
                _ = self.status.changed() => {}
                () = tokio::time::sleep(self.r.timings.idle_poll) => {}
            }
        }
    }

    pub(super) async fn finish(mut self, result: Result<(), Failure>) {
        let stopped = self.permit.stop_requested();
        let mut result = result;
        if let Err(f) = &mut result
            && f.from_klipper
            && !stopped
        {
            self.refresh_klipper_message(f).await;
        }
        // Only Klipper's own shutdown proves nothing is moving. A stop that was requested but
        // not confirmed, or a command that never answered, leaves the motion uncertain.
        let confirmed_down = matches!(
            self.status.borrow().klippy.state,
            KlippyState::Shutdown | KlippyState::Error
        );
        let failure = result.err().map(|mut f| {
            if stopped {
                f.reason = "emergency stop".to_owned();
            }
            f.uncertain = (f.uncertain || f.in_flight) && !confirmed_down;
            f
        });
        // An emergency stop shuts Klipper down, so a restore that fails after one is expected. The
        // restart the stop already needs brings the settings back, and asking for another one is noise.
        let needs_restart = self.restore_settings(confirmed_down).await && !stopped;
        self.publish(Phase::Sealing);
        if failure.is_some() {
            self.add_interrupted_segment().await;
        }
        let summaries: Vec<_> = self.recorders.iter().map(Recorder::summary).collect();
        // Closing the bridges comes before sealing, so the chips stop streaming at once.
        self.recorders.clear();

        let reason = failure.as_ref().map(|f| f.reason.clone());
        let code = failure.as_ref().and_then(|f| f.code);
        let segments = self.segments_stored();
        let mut finished = match self.stored.take() {
            Some(Stored { capture, meta }) if segments > 0 => {
                let sealing = Sealing {
                    id: self.id.clone(),
                    kind: self.plan.kind,
                    meta,
                    summaries,
                    reason,
                    incomplete: std::mem::take(&mut self.incomplete),
                };
                seal(&self.r.storage, capture, sealing).await
            }
            other => {
                let capture = other.and_then(|stored| stored.capture);
                abandon(&self.r.storage, &self.id, self.plan.kind, capture, reason).await
            }
        };
        finished.code = code;
        finished.needs_restart = needs_restart;
        self.end(finished, failure.is_some_and(|f| f.uncertain));
    }

    pub(super) fn end(self, mut finished: FinishedTest, uncertain: bool) {
        let max = self.r.max_segment_seconds;
        if uncertain {
            self.r.slot.start_quarantine(quarantine_duration(max));
            self.r.slot.set_motion_uncertain(&finished.id);
            finished.motion_uncertain = true;
            watch_idle(
                self.r.moonraker.clone(),
                self.r.slot.clone(),
                self.r.hub.clone(),
                self.r.timings,
                finished.id.clone(),
            );
        }
        self.r.hub.update(|s| {
            s.test = None;
            s.last = Some(finished);
        });
        // The permit drops with `self`, after the hub shows the end.
    }
}
