//! The test runner: one task that runs the segments in order, records each chip, seals the
//! session and cleans up on every path.
//!
//! The daemon sends no motion command before pre-flight passed, writes each finished segment to
//! disk before the next starts, and ends every test by closing the chip connections, sealing or
//! abandoning the capture and freeing the slot.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::Coverage;
use wobblescope_session::meta::{Outcome, PrinterInfo, SegmentMeta, SessionMeta, TimeWindow};

use super::live::{ChipLive, FinishedOutcome, FinishedTest, LiveHub, Phase, TestState};
use super::meta::{ChipFacts, apply_summaries, initial, slow_rate_note};
use super::position::{PositionError, Timeouts, move_to_point, window_end, window_start};
use super::preflight::{Started, check_printer, quarantine_duration};
use super::recorder::{Level, Missing, Recorder};
use super::request::{Segment, TestPlan};
use super::slot::{SlotPermit, TestSlot};
use crate::moonraker::Moonraker;
use crate::moonraker::rpc::{Rpc, RpcError};
use crate::moonraker::status::{KlippyState, Status};
use crate::storage::capture::Capture;
use crate::storage::{Storage, StorageError};

/// The seconds added to `max_segment_seconds` for the sweep call's own timeout.
const SWEEP_TIMEOUT_MARGIN_SECONDS: u64 = 60;

/// The longest a finished test stays motion-uncertain.
const UNCERTAIN_CAP: Duration = Duration::from_secs(30 * 60);

/// The runner's timings. Tests set them to milliseconds.
#[derive(Debug, Clone, Copy)]
pub struct RunnerTimings {
    /// How often progress is published while a sweep runs.
    pub poll: Duration,
    /// How long every chip may take to deliver data past a window end.
    pub drain: Duration,
    /// How long to wait for the toolhead to leave `Printing` between segments.
    pub idle_wait: Duration,
    /// The poll interval of that wait.
    pub idle_poll: Duration,
    /// How long a stopped or shut-down sweep call may keep the runner waiting.
    pub shutdown_grace: Duration,
    /// Timeouts of the positioning calls.
    pub positioning: Timeouts,
    /// The sweep call's timeout; `None` means `max_segment_seconds` plus a minute.
    pub sweep_timeout: Option<Duration>,
    /// How long a motion-uncertain test stays so at most.
    pub uncertain_cap: Duration,
}

impl Default for RunnerTimings {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(500),
            drain: Duration::from_secs(2),
            idle_wait: Duration::from_secs(3),
            idle_poll: Duration::from_millis(250),
            shutdown_grace: Duration::from_secs(2),
            positioning: Timeouts::default(),
            sweep_timeout: None,
            uncertain_cap: UNCERTAIN_CAP,
        }
    }
}

/// What the runner works with. Clones share it.
#[derive(Clone)]
pub struct Runner {
    /// Moonraker.
    pub moonraker: Moonraker,
    /// Storage, already open.
    pub storage: Storage,
    /// The slot and quarantine.
    pub slot: TestSlot,
    /// The live state.
    pub hub: LiveHub,
    /// The longest one sweep command may run, in seconds.
    pub max_segment_seconds: u32,
    /// Timings.
    pub timings: RunnerTimings,
}

/// A test ready to run.
pub struct Job {
    /// What pre-flight returned.
    pub started: Started,
    /// The validated request.
    pub plan: TestPlan,
    /// The slot.
    pub permit: SlotPermit,
}

impl Runner {
    /// Runs `job` on its own task.
    #[must_use]
    pub fn spawn(&self, job: Job) -> JoinHandle<()> {
        let run = Run::new(self.clone(), job);
        tokio::spawn(run.run())
    }
}

/// Why a test stopped before its last segment.
#[derive(Debug, Clone)]
struct Failure {
    reason: String,
    /// Whether a command whose outcome is unknown may still be moving the toolhead.
    uncertain: bool,
    /// Whether the reason comes from Klipper's state, whose message may arrive a moment after
    /// the state itself.
    from_klipper: bool,
    /// Whether the runner stopped waiting for a motion command that had not answered.
    in_flight: bool,
}

impl Failure {
    fn certain(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            uncertain: false,
            from_klipper: false,
            in_flight: false,
        }
    }

    fn unknown(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            uncertain: true,
            from_klipper: false,
            in_flight: false,
        }
    }

    /// Marks that the runner gave up waiting for a command that had not answered.
    fn left_in_flight(mut self) -> Self {
        self.in_flight = true;
        self
    }

    fn klipper(reason: impl Into<String>) -> Self {
        Self {
            from_klipper: true,
            ..Self::certain(reason)
        }
    }
}

impl From<PositionError> for Failure {
    fn from(e: PositionError) -> Self {
        match e {
            PositionError::Unknown(_) => Self::unknown(e.to_string()),
            _ => Self::certain(e.to_string()),
        }
    }
}

struct Current {
    index: usize,
    /// The window start, once the positioning is proven.
    start: Option<f64>,
    began: Instant,
}

struct Run {
    r: Runner,
    plan: TestPlan,
    id: String,
    created_at: String,
    permit: SlotPermit,
    chips: Vec<crate::capture::availability::OpenChip>,
    reservation: Option<crate::storage::Reservation>,
    recorders: Vec<Recorder>,
    levels: mpsc::UnboundedReceiver<Level>,
    levels_tx: Option<mpsc::UnboundedSender<Level>>,
    chip_live: Vec<ChipLive>,
    capture: Option<Capture>,
    meta: Option<SessionMeta>,
    current: Option<Current>,
    incomplete: Vec<String>,
    status: watch::Receiver<Status>,
}

impl Run {
    fn new(r: Runner, job: Job) -> Self {
        let Job {
            started,
            plan,
            permit,
        } = job;
        let (levels_tx, levels) = mpsc::unbounded_channel();
        let status = r.moonraker.watch_status();
        let chip_live = started
            .chips
            .iter()
            .map(|c| ChipLive {
                chip: c.chip.section.clone(),
                level: None,
                warnings: Vec::new(),
            })
            .collect();
        Self {
            r,
            plan,
            id: started.id,
            created_at: started.created_at,
            permit,
            chips: started.chips,
            reservation: Some(started.reservation),
            recorders: Vec::new(),
            levels,
            levels_tx: Some(levels_tx),
            chip_live,
            capture: None,
            meta: None,
            current: None,
            incomplete: Vec::new(),
            status,
        }
    }

    async fn run(mut self) {
        self.permit.begin(&self.id);
        let result = self.execute().await;
        self.finish(result).await;
    }

    // ---- publishing ----

    fn publish(&mut self, phase: Phase) {
        while let Ok(level) = self.levels.try_recv() {
            if let Some(chip) = self.chip_live.iter_mut().find(|c| c.chip == level.chip) {
                chip.level = Some(level.rms);
            }
        }
        let segment = self
            .current
            .as_ref()
            .map(|c| (c, self.plan.segments[c.index]));
        let state = TestState {
            id: self.id.clone(),
            phase,
            segment: segment.map_or(0, |(c, _)| c.index as u32 + 1),
            segments: self.plan.segments.len() as u32,
            axis: segment.map(|(_, s)| s.axis),
            position: segment.map(|(_, s)| s.position),
            expected_segment_seconds: self.plan.segment_seconds,
            elapsed_seconds: segment.map_or(0.0, |(c, _)| c.began.elapsed().as_secs_f64()),
            chips: self.chip_live.clone(),
        };
        self.r.hub.update(|s| s.test = Some(state));
    }

    fn warn(&mut self, chip: &str, note: &str) {
        if let Some(c) = self.chip_live.iter_mut().find(|c| c.chip == chip)
            && !c.warnings.iter().any(|w| w == note)
        {
            c.warnings.push(note.to_owned());
        }
    }

    // ---- the test ----

    async fn execute(&mut self) -> Result<(), Failure> {
        let facts: Vec<ChipFacts> = self
            .chips
            .iter()
            .map(|c| ChipFacts::of(&c.chip, c.observed_rate_hz))
            .collect();
        for fact in &facts {
            if let Some(note) = slow_rate_note(fact.nominal_rate_hz, fact.observed_rate_hz) {
                self.warn(&fact.section, &note);
            }
        }
        let control = self.control()?;
        let printer = printer_info(&control).await;
        let meta = initial(&self.plan, &self.id, &self.created_at, &facts, printer);

        let reservation = self.reservation.take().expect("reserved by pre-flight");
        let storage = self.r.storage.clone();
        let begin_meta = meta.clone();
        let capture =
            tokio::task::spawn_blocking(move || storage.begin_capture(reservation, &begin_meta))
                .await
                .map_err(|e| Failure::certain(format!("the storage task failed: {e}")))?
                .map_err(|e| Failure::certain(format!("cannot start the session: {e}")))?;
        self.capture = Some(capture);
        self.meta = Some(meta);

        let tx = self.levels_tx.take();
        for open in std::mem::take(&mut self.chips) {
            self.recorders.push(Recorder::spawn(open, tx.clone()));
        }

        for index in 0..self.plan.segments.len() {
            let segment = self.plan.segments[index];
            self.current = Some(Current {
                index,
                start: None,
                began: Instant::now(),
            });
            self.publish(Phase::Positioning);
            if index > 0 {
                self.recheck().await?;
            }
            self.check_stop()?;
            self.check_chips_alive()?;
            for rec in &self.recorders {
                rec.begin_segment();
            }
            self.position(&segment).await?;
            let control = self.control()?;
            let start = window_start(&control, self.r.timings.positioning.query).await?;
            if let Some(c) = &mut self.current {
                c.start = Some(start);
            }
            self.publish(Phase::Measuring);
            self.sweep(&segment).await?;
            let control = self.control()?;
            let end = window_end(&control, self.r.timings.positioning.query)
                .await
                .map_err(|e| Failure::certain(format!("cannot read the window end: {e}")))?;
            self.drain_and_store(&segment, start, end).await?;
            self.current = None;
        }
        Ok(())
    }

    fn control(&self) -> Result<Rpc, Failure> {
        self.r
            .moonraker
            .control()
            .ok_or_else(|| Failure::certain("the connection to Moonraker was lost"))
    }

    fn check_stop(&self) -> Result<(), Failure> {
        if self.permit.stop_requested() {
            Err(Failure::certain("emergency stop"))
        } else {
            Ok(())
        }
    }

    /// A failure that carries Klipper's message, when Klipper is not ready.
    fn klipper_down(&self) -> Option<Failure> {
        let status = self.status.borrow();
        (status.klippy.state != KlippyState::Ready).then(|| {
            Failure::klipper(
                status
                    .klippy
                    .message
                    .clone()
                    .unwrap_or_else(|| format!("Klipper is {}", status.klippy.state.as_str())),
            )
        })
    }

    fn check_chips_alive(&self) -> Result<(), Failure> {
        for rec in &self.recorders {
            if let Some(reason) = rec.closed() {
                return Err(Failure::certain(format!(
                    "the capture connection of {} was lost: {reason}",
                    rec.name()
                )));
            }
        }
        Ok(())
    }

    /// Repeats the pre-flight checks that another client can invalidate between segments. The
    /// idle check waits a little first, because the toolhead stays `Printing` for about half a
    /// second after the previous segment's last move.
    async fn recheck(&mut self) -> Result<(), Failure> {
        let waited = Instant::now();
        loop {
            self.check_stop()?;
            if let Some(failure) = self.klipper_down() {
                return Err(failure);
            }
            let failures = check_printer(&self.r.moonraker, false).await;
            if failures.is_empty() {
                return Ok(());
            }
            let only_idle = failures.iter().all(|f| f.check == "toolhead_idle");
            if only_idle && waited.elapsed() < self.r.timings.idle_wait {
                tokio::time::sleep(self.r.timings.idle_poll).await;
                continue;
            }
            let reasons: Vec<String> = failures.into_iter().map(|f| f.message).collect();
            return Err(Failure::certain(reasons.join("; ")));
        }
    }

    /// Waits until the stop flag is set or Klipper leaves ready.
    async fn interrupted(&mut self) -> Failure {
        loop {
            if self.permit.stop_requested() {
                return Failure::certain("emergency stop");
            }
            if let Some(failure) = self.klipper_down() {
                return failure;
            }
            tokio::select! {
                _ = self.status.changed() => {}
                () = tokio::time::sleep(Duration::from_millis(25)) => {}
            }
        }
    }

    async fn position(&mut self, segment: &Segment) -> Result<(), Failure> {
        let control = self.control()?;
        let moonraker = self.r.moonraker.clone();
        let speed = self.plan.effective.move_speed;
        let timeouts = self.r.timings.positioning;
        let moving = move_to_point(&control, segment.position, speed, timeouts, move || {
            moonraker.status().klippy.state == KlippyState::Ready
        });
        tokio::pin!(moving);
        tokio::select! {
            result = &mut moving => result.map_err(Failure::from),
            failure = self.interrupted() => Err(failure.left_in_flight()),
        }
    }

    /// Runs the sweep and waits for it, publishing progress, watching for Klipper's shutdown and
    /// a stop, and recording a lost chip without leaving the slot while the toolhead may move.
    async fn sweep(&mut self, segment: &Segment) -> Result<(), Failure> {
        let control = self.control()?;
        let command = self.plan.command(segment, &self.id);
        let timeout = self.r.timings.sweep_timeout.unwrap_or_else(|| {
            Duration::from_secs(
                u64::from(self.r.max_segment_seconds) + SWEEP_TIMEOUT_MARGIN_SECONDS,
            )
        });
        let call = control.call(
            "printer.gcode.script",
            Some(json!({"script": format!("{command}\nM400")})),
            timeout,
        );
        tokio::pin!(call);
        let mut tick = tokio::time::interval(self.r.timings.poll);
        let mut recorded: Option<String> = None;
        let mut halted: Option<(Instant, Failure)> = None;
        loop {
            tokio::select! {
                result = &mut call => {
                    return match result {
                        Ok(_) => recorded.map_or(Ok(()), |r| Err(Failure::certain(r))),
                        Err(RpcError::Moonraker { message, .. }) => {
                            // Klipper's own message beats the generic error of the call.
                            Err(match recorded {
                                Some(reason) => Failure::certain(reason),
                                None => self.klipper_down().unwrap_or_else(|| Failure::certain(message)),
                            })
                        }
                        Err(e) => Err(Failure::unknown(format!(
                            "the sweep's outcome is unknown: {e}"
                        ))),
                    };
                }
                _ = tick.tick() => {}
                _ = self.status.changed() => {}
            }
            if halted.is_none() {
                if self.permit.stop_requested() {
                    halted = Some((Instant::now(), Failure::certain("emergency stop")));
                } else if let Some(failure) = self.klipper_down() {
                    halted = Some((Instant::now(), failure));
                }
            }
            if let Some((since, failure)) = &halted
                && since.elapsed() >= self.r.timings.shutdown_grace
            {
                // Whether anything still moves depends on Klipper: `finish` decides.
                return Err(failure.clone().left_in_flight());
            }
            if recorded.is_none()
                && let Err(failure) = self.check_chips_alive()
            {
                recorded = Some(failure.reason);
            }
            self.publish(if recorded.is_some() {
                Phase::WaitingForSweep
            } else {
                Phase::Measuring
            });
        }
    }

    /// Waits for every chip's data to pass the window end, takes the segment and writes it.
    async fn drain_and_store(
        &mut self,
        segment: &Segment,
        start: f64,
        end: f64,
    ) -> Result<(), Failure> {
        self.publish(Phase::Draining);
        let waits = self
            .recorders
            .iter()
            .map(|r| r.wait_past(end, self.r.timings.drain));
        let results = futures_util::future::join_all(waits).await;
        let mut failure = None;
        let mut recordings: Vec<(u32, RawSamples, Coverage)> = Vec::new();
        let index = self.meta.as_ref().map_or(0, |m| m.segments.len());
        let mut notes: Vec<(String, String)> = Vec::new();
        for (chip, (rec, result)) in self.recorders.iter().zip(results).enumerate() {
            match result {
                Ok(()) => {}
                Err(Missing::NoData) => {}
                Err(Missing::Closed(reason)) => {
                    failure.get_or_insert_with(|| {
                        Failure::certain(format!(
                            "the capture connection of {} was lost: {reason}",
                            rec.name()
                        ))
                    });
                }
            }
            let taken = rec.take(start, end);
            for note in &taken.report.notes {
                notes.push((rec.name().to_owned(), note.clone()));
            }
            if let Coverage::Incomplete { missing } = &taken.report.coverage {
                self.incomplete.push(format!(
                    "{} in segment {}: {missing}",
                    rec.name(),
                    index + 1
                ));
            }
            if taken.samples.is_empty() {
                continue;
            }
            recordings.push((chip as u32, taken.samples, taken.report.coverage));
        }
        for (chip, note) in notes {
            self.warn(&chip, &note);
        }
        let window = TimeWindow { start, end };
        self.store_segment(segment, index, recordings, window)
            .await?;
        failure.map_or(Ok(()), Err)
    }

    async fn store_segment(
        &mut self,
        segment: &Segment,
        index: usize,
        recordings: Vec<(u32, RawSamples, Coverage)>,
        window: TimeWindow,
    ) -> Result<(), Failure> {
        let files = self
            .with_capture(move |capture| {
                let refs: Vec<(u32, &RawSamples, Coverage)> = recordings
                    .iter()
                    .map(|(c, s, cov)| (*c, s, cov.clone()))
                    .collect();
                capture.write_segment(index, &refs)
            })
            .await
            .map_err(|e| Failure::certain(format!("disk write failed: {e}")))?;
        let meta = self.meta.as_mut().expect("the capture has metadata");
        meta.segments.push(SegmentMeta {
            axis: segment.axis,
            point: segment.point as u32,
            window: Some(window),
            files,
        });
        let journal = meta.clone();
        self.with_capture(move |capture| capture.journal(&journal))
            .await
            .map_err(|e| Failure::certain(format!("disk write failed: {e}")))
    }

    /// Runs `f` on a blocking thread with the capture.
    async fn with_capture<T: Send + 'static>(
        &mut self,
        f: impl FnOnce(&mut Capture) -> Result<T, StorageError> + Send + 'static,
    ) -> Result<T, StorageError> {
        let mut capture = self
            .capture
            .take()
            .ok_or_else(|| StorageError::BadSession("the capture is not open".to_owned()))?;
        let (capture, result) = tokio::task::spawn_blocking(move || {
            let result = f(&mut capture);
            (capture, result)
        })
        .await
        .map_err(|e| StorageError::BadSession(format!("the storage task failed: {e}")))?;
        self.capture = Some(capture);
        result
    }

    // ---- the end ----

    /// Adds the segment that was running when the test failed, as incomplete, when at least one
    /// chip delivered a sample at or after its window start.
    async fn add_interrupted_segment(&mut self) {
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
        let mut recordings = Vec::new();
        for (chip, rec) in self.recorders.iter().enumerate() {
            let taken = rec.take(start, end);
            if taken.samples.t.iter().any(|t| *t >= start) {
                let coverage = Coverage::Incomplete {
                    missing: "the test was interrupted".to_owned(),
                };
                recordings.push((chip as u32, taken.samples, coverage));
            }
        }
        if recordings.is_empty() {
            return;
        }
        let segment = self.plan.segments[index];
        let at = self.meta.as_ref().map_or(0, |m| m.segments.len());
        self.incomplete
            .push(format!("segment {} was interrupted", at + 1));
        // A failure to write it leaves the finished segments, which are already journaled.
        let _ = self
            .store_segment(&segment, at, recordings, TimeWindow { start, end })
            .await;
    }

    /// Klipper's state changes before its message arrives. Waits a little for the message so the
    /// session says why Klipper stopped.
    async fn refresh_klipper_message(&mut self, failure: &mut Failure) {
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

    async fn finish(mut self, result: Result<(), Failure>) {
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
        self.publish(Phase::Sealing);
        if failure.is_some() {
            self.add_interrupted_segment().await;
        }
        let summaries: Vec<_> = self.recorders.iter().map(Recorder::summary).collect();
        // Closing the bridges comes before sealing, so the chips stop streaming at once.
        self.recorders.clear();
        self.chips.clear();

        let reason = failure.as_ref().map(|f| f.reason.clone());
        let segments = self.meta.as_ref().map_or(0, |m| m.segments.len());
        let finished = if segments == 0 {
            if let Some(capture) = self.capture.take() {
                let storage = self.r.storage.clone();
                let _ = tokio::task::spawn_blocking(move || storage.abandon_capture(capture)).await;
            }
            FinishedTest {
                id: self.id.clone(),
                outcome: FinishedOutcome::NoSession,
                reason: Some(reason.unwrap_or_else(|| "no segment finished".to_owned())),
                session_id: None,
                motion_uncertain: false,
            }
        } else {
            self.seal(summaries, reason).await
        };
        self.end(finished, failure.is_some_and(|f| f.uncertain));
    }

    async fn seal(
        &mut self,
        summaries: Vec<super::recorder::ChipSummary>,
        reason: Option<String>,
    ) -> FinishedTest {
        let mut meta = self.meta.take().expect("segments imply metadata");
        apply_summaries(&mut meta, &summaries);
        let (outcome, kind) = match &reason {
            Some(r) => {
                meta.notes.push(r.clone());
                (
                    Outcome::Failed { reason: r.clone() },
                    FinishedOutcome::Failed,
                )
            }
            None if !self.incomplete.is_empty() => (
                Outcome::Incomplete {
                    reason: self.incomplete.join("; "),
                },
                FinishedOutcome::Incomplete,
            ),
            None => (Outcome::Complete, FinishedOutcome::Complete),
        };
        meta.notes.extend(self.incomplete.iter().cloned());
        meta.outcome = outcome;
        let finished = |kind, reason: Option<String>, session: Option<String>| FinishedTest {
            id: self.id.clone(),
            outcome: kind,
            reason,
            session_id: session,
            motion_uncertain: false,
        };
        let Some(capture) = self.capture.take() else {
            return finished(
                FinishedOutcome::Failed,
                Some("could not seal: the capture is not open".to_owned()),
                None,
            );
        };
        let storage = self.r.storage.clone();
        let sealed = tokio::task::spawn_blocking(move || {
            let result = storage.seal_capture(&capture, &meta);
            // The capture drops here, releasing its reservation either way.
            result.map(|summary| summary.id)
        })
        .await;
        match sealed {
            Ok(Ok(id)) => finished(
                kind,
                reason.or_else(|| {
                    (kind == FinishedOutcome::Incomplete).then(|| self.incomplete.join("; "))
                }),
                Some(id),
            ),
            Ok(Err(e)) => finished(
                FinishedOutcome::Failed,
                Some(format!(
                    "could not seal: {e}; the next daemon start recovers the finished segments"
                )),
                None,
            ),
            Err(e) => finished(
                FinishedOutcome::Failed,
                Some(format!("could not seal: the storage task failed: {e}")),
                None,
            ),
        }
    }

    /// Publishes the end, starts the quarantine after a command of unknown outcome and frees the
    /// slot.
    fn end(self, mut finished: FinishedTest, uncertain: bool) {
        let max = self.r.max_segment_seconds;
        if uncertain {
            self.r.slot.start_quarantine(quarantine_duration(max));
            self.r.slot.set_motion_uncertain(&finished.id);
            finished.motion_uncertain = true;
            watch_idle(self.r.clone(), finished.id.clone());
        }
        self.r.hub.update(|s| {
            s.test = None;
            s.last = Some(finished);
        });
        // The permit drops with `self`, after the hub shows the end.
    }
}

/// Clears the motion-uncertain marker once the toolhead is seen idle twice, or after the cap.
fn watch_idle(r: Runner, id: String) {
    tokio::spawn(async move {
        let deadline = Instant::now() + r.timings.uncertain_cap;
        let mut idle_reads = 0;
        while Instant::now() < deadline {
            tokio::time::sleep(r.timings.idle_poll).await;
            let idle = match r.moonraker.control() {
                Some(control) => match super::preflight::read_printer_state(&control).await {
                    Ok(state) => state.idle_state.is_some_and(|s| s != "Printing"),
                    Err(_) => false,
                },
                None => false,
            };
            idle_reads = if idle { idle_reads + 1 } else { 0 };
            if idle_reads >= 2 {
                break;
            }
        }
        if r.slot.motion_uncertain().as_deref() == Some(id.as_str()) {
            r.slot.clear_motion_uncertain();
        }
        r.hub.update(|s| {
            if let Some(last) = s.last.as_mut().filter(|l| l.id == id) {
                last.motion_uncertain = false;
            }
        });
    });
}

/// The printer's host name and Klipper version, best effort.
async fn printer_info(control: &Rpc) -> Option<PrinterInfo> {
    let info: Value = control
        .call("printer.info", None, Duration::from_secs(5))
        .await
        .ok()?;
    let text = |key: &str| info.get(key).and_then(Value::as_str).map(str::to_owned);
    Some(PrinterInfo {
        name: text("hostname"),
        klipper_version: text("software_version"),
        mcu_firmware: Vec::new(),
    })
}
