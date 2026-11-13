//! The test runner: one task that runs the segments in order, records each chip, seals the
//! session and cleans up on every path.
//!
//! The daemon sends no motion command before pre-flight passed, writes each finished segment to
//! disk before the next starts, and ends every test by closing the chip connections, sealing or
//! abandoning the capture and freeing the slot.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use wobblescope_core::RawSamples;
use wobblescope_core::direction::Direction;
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_session::meta::{
    PrinterInfo, SegmentMeta, SessionMeta, TimeWindow, undersampled_note,
};

use super::availability::OpenChip;
use super::live::{ChipLive, FailureCode, FinishedTest, LiveHub, Phase, TestState};
use super::meta::{ChipFacts, initial};
use super::position::{PositionError, Timeouts, move_to_point, window_end, window_start};
use super::preflight::{Started, check_printer, quarantine_duration};
use super::recorder::{Level, Missing, Recorder, Taken};
use super::request::{Segment, TestPlan, diagonal_refusal};
use super::seal::{Sealing, abandon, seal};
use super::slot::{SlotPermit, TestSlot};
use crate::moonraker::Moonraker;
use crate::moonraker::rpc::{Rpc, RpcError};
use crate::moonraker::settings::{Shaping, parse_shaping};
use crate::moonraker::status::{KlippyState, Status};
use crate::storage::Reservation;
use crate::storage::capture::Capture;
use crate::storage::{Storage, StorageError, run_blocking};

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
        let (run, setup) = Run::new(self.clone(), job);
        // Named before the task is polled, so a stop right after the 202 finds the test.
        run.permit.begin(&run.id);
        tokio::spawn(run.run(setup))
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
    code: Option<FailureCode>,
}

impl Failure {
    fn certain(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            uncertain: false,
            from_klipper: false,
            in_flight: false,
            code: None,
        }
    }

    fn unknown(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            uncertain: true,
            from_klipper: false,
            in_flight: false,
            code: None,
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

#[derive(Default)]
struct SegmentOutcome {
    recordings: Vec<(u32, RawSamples, Coverage)>,
    judged: Vec<(u32, Health, Vec<String>)>,
}

impl SegmentOutcome {
    fn add(&mut self, chip: usize, taken: Taken, coverage: Coverage) {
        let chip = chip as u32;
        self.judged
            .push((chip, taken.report.health, taken.report.notes));
        self.recordings.push((chip, taken.samples, coverage));
    }
}

/// What only the start of a test needs. `execute` consumes it.
struct Setup {
    reservation: Reservation,
    chips: Vec<OpenChip>,
    levels_tx: mpsc::UnboundedSender<Level>,
}

/// The session on disk once the capture has begun. The capture is lent to a blocking thread while
/// a write runs, and is `None` only then or after that thread failed.
struct Stored {
    capture: Option<Capture>,
    meta: SessionMeta,
}

/// What the live socket shows about the chips. Signal levels come from the recorders and
/// warnings from the runner.
struct Progress {
    levels: mpsc::UnboundedReceiver<Level>,
    chips: Vec<ChipLive>,
    console: broadcast::Receiver<String>,
    frequency: Option<f64>,
}

/// The frequency in a console line such as `// Testing frequency 57 Hz`.
fn tested_frequency(line: &str) -> Option<f64> {
    let rest = line.split_once("Testing frequency ")?.1;
    rest.split_whitespace().next()?.parse().ok()
}

impl Progress {
    fn restart_frequency(&mut self) {
        self.frequency = None;
        while self.console.try_recv().is_ok() {}
    }

    fn frequency(&mut self) -> Option<f64> {
        loop {
            match self.console.try_recv() {
                Ok(line) => {
                    if let Some(hz) = tested_frequency(&line) {
                        self.frequency = Some(hz);
                    }
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                Err(_) => return self.frequency,
            }
        }
    }

    fn snapshot(&mut self) -> Vec<ChipLive> {
        while let Ok(level) = self.levels.try_recv() {
            if let Some(chip) = self.chips.iter_mut().find(|c| c.chip == level.chip) {
                chip.level = Some(level.rms);
            }
        }
        self.chips.clone()
    }

    fn warn(&mut self, chip: &str, note: &str) {
        if let Some(c) = self.chips.iter_mut().find(|c| c.chip == chip)
            && !c.warnings.iter().any(|w| w == note)
        {
            c.warnings.push(note.to_owned());
        }
    }
}

struct Run {
    r: Runner,
    plan: TestPlan,
    id: String,
    created_at: String,
    permit: SlotPermit,
    recorders: Vec<Recorder>,
    progress: Progress,
    stored: Option<Stored>,
    current: Option<Current>,
    incomplete: Vec<String>,
    status: watch::Receiver<Status>,
}

impl Run {
    fn new(r: Runner, job: Job) -> (Self, Setup) {
        let Job {
            started,
            plan,
            permit,
        } = job;
        let (levels_tx, levels) = mpsc::unbounded_channel();
        let status = r.moonraker.watch_status();
        let console = r.moonraker.gcode_lines();
        let chips = started
            .chips
            .iter()
            .map(|c| ChipLive {
                chip: c.chip.section.clone(),
                level: None,
                warnings: Vec::new(),
            })
            .collect();
        let run = Self {
            r,
            plan,
            id: started.id,
            created_at: started.created_at,
            permit,
            recorders: Vec::new(),
            progress: Progress {
                levels,
                chips,
                console,
                frequency: None,
            },
            stored: None,
            current: None,
            incomplete: Vec::new(),
            status,
        };
        let setup = Setup {
            reservation: started.reservation,
            chips: started.chips,
            levels_tx,
        };
        (run, setup)
    }

    async fn run(mut self, setup: Setup) {
        let result = self.execute(setup).await;
        self.finish(result).await;
    }

    // ---- publishing ----

    fn publish(&mut self, phase: Phase) {
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
            frequency_hz: self.progress.frequency(),
            frequency_range: [self.plan.effective.freq_start, self.plan.effective.freq_end],
            chips: self.progress.snapshot(),
        };
        self.r.hub.update(|s| s.test = Some(state));
    }

    fn warn(&mut self, chip: &str, note: &str) {
        self.progress.warn(chip, note);
    }

    // ---- the test ----

    async fn execute(&mut self, setup: Setup) -> Result<(), Failure> {
        let Setup {
            reservation,
            chips,
            levels_tx,
        } = setup;
        let facts: Vec<ChipFacts> = chips
            .iter()
            .map(|c| ChipFacts::of(&c.chip, c.observed_rate_hz))
            .collect();
        for fact in &facts {
            if let Some(note) = undersampled_note(fact.nominal_rate_hz, Some(fact.observed_rate_hz))
            {
                self.warn(&fact.section, &note);
            }
        }
        let control = self.control()?;
        let mut mcus: Vec<String> = facts.iter().filter_map(|f| f.mcu.clone()).collect();
        let printer = printer_info(&control, &mut mcus).await;
        let meta = initial(&self.plan, &self.id, &self.created_at, &facts, printer);

        let storage = self.r.storage.clone();
        let begin_meta = meta.clone();
        let capture = run_blocking(move || storage.begin_capture(reservation, &begin_meta))
            .await
            .map_err(Failure::certain)?
            .map_err(|e| Failure::certain(format!("cannot start the session: {e}")))?;
        self.stored = Some(Stored {
            capture: Some(capture),
            meta,
        });

        for open in chips {
            self.recorders
                .push(Recorder::spawn(open, Some(levels_tx.clone())));
        }

        for index in 0..self.plan.segments.len() {
            let segment = self.plan.segments[index];
            self.current = Some(Current {
                index,
                start: None,
                began: Instant::now(),
            });
            self.progress.restart_frequency();
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
                                None => self.klipper_down().unwrap_or_else(|| {
                                    let refusal = matches!(segment.axis, Direction::Vector(_))
                                        .then(|| diagonal_refusal(&message))
                                        .flatten();
                                    match refusal {
                                        Some(text) => Failure {
                                            code: Some(FailureCode::DiagonalAxisRefused),
                                            ..Failure::certain(text)
                                        },
                                        None => Failure::certain(message),
                                    }
                                }),
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
        let mut outcome = SegmentOutcome::default();
        let index = self.segments_stored();
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
            let coverage = taken.report.coverage.clone();
            outcome.add(chip, taken, coverage);
        }
        for (chip, note) in notes {
            self.warn(&chip, &note);
        }
        let window = TimeWindow { start, end };
        self.store_segment(segment, index, outcome, window).await?;
        failure.map_or(Ok(()), Err)
    }

    async fn store_segment(
        &mut self,
        segment: &Segment,
        index: usize,
        outcome: SegmentOutcome,
        window: TimeWindow,
    ) -> Result<(), Failure> {
        let SegmentOutcome { recordings, judged } = outcome;
        let mut files = self
            .with_capture(move |capture| {
                let refs: Vec<(u32, &RawSamples, Coverage)> = recordings
                    .iter()
                    .map(|(c, s, cov)| (*c, s, cov.clone()))
                    .collect();
                capture.write_segment(index, &refs)
            })
            .await
            .map_err(|e| Failure::certain(format!("disk write failed: {e}")))?;
        for file in &mut files {
            if let Some((_, health, notes)) = judged.iter().find(|(chip, _, _)| *chip == file.chip)
            {
                file.health = Some(*health);
                file.health_notes.clone_from(notes);
            }
        }
        let Some(stored) = self.stored.as_mut() else {
            return Err(Failure::certain("the capture is not open"));
        };
        stored.meta.segments.push(SegmentMeta {
            axis: segment.axis,
            point: segment.point as u32,
            window: Some(window),
            files,
        });
        // Version 2 is written only once a vector segment is in the journal, so a session with
        // none stays readable by a daemon that predates belt tests.
        stored.meta.format_version = stored.meta.required_version();
        let journal = stored.meta.clone();
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
            .stored
            .as_mut()
            .and_then(|stored| stored.capture.take())
            .ok_or_else(|| StorageError::BadSession("the capture is not open".to_owned()))?;
        let (capture, result) = run_blocking(move || {
            let result = f(&mut capture);
            (capture, result)
        })
        .await
        .map_err(StorageError::BadSession)?;
        if let Some(stored) = self.stored.as_mut() {
            stored.capture = Some(capture);
        }
        result
    }

    fn segments_stored(&self) -> usize {
        self.stored.as_ref().map_or(0, |s| s.meta.segments.len())
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
            .store_segment(&segment, at, outcome, TimeWindow { start, end })
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

        let reason = failure.as_ref().map(|f| f.reason.clone());
        let code = failure.as_ref().and_then(|f| f.code);
        let segments = self.segments_stored();
        let mut finished = match self.stored.take() {
            Some(Stored { capture, meta }) if segments > 0 => {
                let sealing = Sealing {
                    id: self.id.clone(),
                    meta,
                    summaries,
                    reason,
                    incomplete: std::mem::take(&mut self.incomplete),
                };
                seal(&self.r.storage, capture, sealing).await
            }
            other => {
                let capture = other.and_then(|stored| stored.capture);
                abandon(&self.r.storage, &self.id, capture, reason).await
            }
        };
        finished.code = code;
        self.end(finished, failure.is_some_and(|f| f.uncertain));
    }

    /// Publishes the end, starts the quarantine after a command of unknown outcome and frees the
    /// slot.
    fn end(self, mut finished: FinishedTest, uncertain: bool) {
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

/// Clears the motion-uncertain marker once the toolhead is seen idle twice, or after the cap.
pub fn watch_idle(
    moonraker: Moonraker,
    slot: TestSlot,
    hub: LiveHub,
    timings: RunnerTimings,
    id: String,
) {
    tokio::spawn(async move {
        let deadline = Instant::now() + timings.uncertain_cap;
        let mut idle_reads = 0;
        while Instant::now() < deadline {
            tokio::time::sleep(timings.idle_poll).await;
            let idle = match moonraker.control() {
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
        if slot.motion_uncertain().as_deref() == Some(id.as_str()) {
            slot.clear_motion_uncertain();
        }
        hub.update(|s| {
            if let Some(last) = s.last.as_mut().filter(|l| l.id == id) {
                last.motion_uncertain = false;
            }
        });
    });
}

/// The printer's host name, Klipper version and the firmware of the MCUs `mcus` names (plus the
/// main MCU), best effort. A failed call leaves what it would have added empty.
async fn printer_info(control: &Rpc, mcus: &mut Vec<String>) -> Option<PrinterInfo> {
    let info: Value = control
        .call("printer.info", None, Duration::from_secs(5))
        .await
        .ok()?;
    let text = |key: &str| info.get(key).and_then(Value::as_str).map(str::to_owned);
    mcus.push("mcu".to_owned());
    mcus.sort();
    mcus.dedup();
    let shaping = read_shaping(control).await;
    Some(PrinterInfo {
        name: text("hostname"),
        klipper_version: text("software_version"),
        mcu_firmware: mcu_firmware(control, mcus).await,
        square_corner_velocity: shaping.square_corner_velocity,
        damping_ratio_x: shaping.damping_ratio_x,
        damping_ratio_y: shaping.damping_ratio_y,
    })
}

/// The printer's square corner velocity and configured damping ratios, best effort: a failed
/// call leaves them unknown.
async fn read_shaping(control: &Rpc) -> Shaping {
    let query = json!({"objects": {
        "toolhead": ["square_corner_velocity"],
        "configfile": ["settings"],
    }});
    match control
        .call(
            "printer.objects.query",
            Some(query),
            Duration::from_secs(15),
        )
        .await
    {
        Ok(answer) => parse_shaping(&answer),
        Err(_) => Shaping::default(),
    }
}

/// `<mcu>: <version>` for each of `mcus`, as Klipper reports it. The main MCU is `mcu`, another
/// is `mcu <name>`. An MCU Klipper does not list is left out.
async fn mcu_firmware(control: &Rpc, mcus: &[String]) -> Vec<String> {
    let key = |name: &str| {
        if name == "mcu" {
            name.to_owned()
        } else {
            format!("mcu {name}")
        }
    };
    let objects: serde_json::Map<String, Value> = mcus
        .iter()
        .map(|name| (key(name), json!(["mcu_version"])))
        .collect();
    let Ok(answer) = control
        .call(
            "printer.objects.query",
            Some(json!({"objects": objects})),
            Duration::from_secs(5),
        )
        .await
    else {
        return Vec::new();
    };
    mcus.iter()
        .filter_map(|name| {
            let version = answer
                .get("status")?
                .get(key(name))?
                .get("mcu_version")?
                .as_str()?;
            Some(format!("{name}: {version}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::tested_frequency;

    #[test]
    fn reads_the_frequency_of_klippers_console_line() {
        assert_eq!(tested_frequency("// Testing frequency 57 Hz"), Some(57.0));
        assert_eq!(tested_frequency("Testing frequency 5 Hz"), Some(5.0));
    }

    #[test]
    fn ignores_every_other_line() {
        assert_eq!(tested_frequency("// Testing axis X"), None);
        assert_eq!(tested_frequency("// Testing frequency soon"), None);
        assert_eq!(tested_frequency(""), None);
    }
}
