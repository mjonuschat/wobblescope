//! One hold: pre-flight, setup, the paced loop, the ending and the restore.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::capture::CheckFailure;
use crate::capture::iso_utc;
use crate::capture::position::{TOLERANCE_MM, Timeouts, position};
use crate::capture::preflight::{check_printer, quarantine_failure};
use crate::capture::slot::{SlotPermit, TestSlot};
use crate::hold::pacer::Pacer;
use crate::hold::pieces::{
    CALIBRATION_MM, DEFAULT_PIECE_SECONDS, Drive, HoldAxis, pairs_for, piece_duration,
    piece_script, ramp,
};
use crate::hold::settings::{Restore, STATE_NAME, Saved, accel_line, restore_script, setup_script};
use crate::hold::travel::segments;
use crate::moonraker::Moonraker;
use crate::moonraker::rpc::Rpc;

/// How long "until stopped" lasts at most, in seconds.
pub const UNTIL_STOPPED_CAP_SECONDS: f64 = 600.0;
/// The duration used when none is given, in seconds.
pub const DEFAULT_DURATION_SECONDS: f64 = 30.0;
/// How often the hold reports its state, in seconds.
const STATE_INTERVAL: f64 = 0.25;
/// How often the slot's stop flag is checked.
const STOP_POLL: Duration = Duration::from_millis(50);
/// The travel speed used when the printer's `move_speed` cannot be read, in mm/s.
const DEFAULT_TRAVEL_SPEED: f64 = 50.0;
/// The start buffer assumed while pacing the travel, before the real one is measured.
const TRAVEL_BUFFER: f64 = 0.4;
/// The longest the toolhead may take to finish the travel after the last segment, in seconds.
const TRAVEL_SETTLE_SECONDS: f64 = 5.0;
/// How long to wait after the calibration move before reading Klipper's print time.
const CALIBRATION_WAIT: Duration = Duration::from_millis(600);

/// What a user asks for.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HoldRequest {
    /// The axis to move along.
    pub axis: HoldAxis,
    /// The frequency, in Hz.
    pub frequency: f64,
    /// Peak acceleration per Hz. The printer's `[resonance_tester]` value when absent.
    #[serde(default)]
    pub accel_per_hz: Option<f64>,
    /// How long to hold, in seconds. `None` means until stopped, up to the cap.
    #[serde(default)]
    pub duration_seconds: Option<f64>,
    /// Where to hold, as X, Y and Z in mm.
    #[serde(default)]
    pub start_point: Option<[f64; 3]>,
}

/// The limits and timings of a hold.
#[derive(Debug, Clone)]
pub struct HoldTimings {
    /// The motion time of one piece, in seconds.
    pub piece_seconds: f64,
    /// The longest "until stopped" lasts, in seconds.
    pub until_stopped_cap_seconds: f64,
    /// The longest one call to Moonraker may take.
    pub call_timeout: Duration,
    /// How long to wait for a call in flight when the hold is ending.
    pub in_flight_wait: Duration,
    /// The timeouts for positioning calls.
    pub timeouts: Timeouts,
}

impl Default for HoldTimings {
    fn default() -> Self {
        Self {
            piece_seconds: DEFAULT_PIECE_SECONDS,
            until_stopped_cap_seconds: UNTIL_STOPPED_CAP_SECONDS,
            call_timeout: Duration::from_secs(5),
            in_flight_wait: Duration::from_secs(2),
            timeouts: Timeouts::default(),
        }
    }
}

/// What a running hold is told.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Change the frequency, in Hz.
    SetFrequency(f64),
    /// Change the axis.
    SetAxis(HoldAxis),
    /// Stop gracefully.
    Stop,
    /// The browser is gone.
    ConnectionLost,
}

/// Where a hold is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum HoldPhase {
    /// Moving to the start point.
    Travelling,
    /// Holding the frequency.
    Holding,
    /// Ending.
    Stopping,
}

/// What a running hold reports.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HoldState {
    /// Where the hold is.
    pub phase: HoldPhase,
    /// Seconds since the hold began.
    pub elapsed_seconds: f64,
    /// The frequency now, in Hz. It ramps toward a changed target.
    pub frequency: f64,
    /// The axis being driven now.
    pub axis: HoldAxis,
    /// The peak acceleration now, in mm/s².
    pub acceleration: f64,
    /// The acceleration limit the hold set, in mm/s².
    pub accel_limit: f64,
}

/// Why a hold ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// The time limit passed.
    TimeLimit,
    /// The user stopped it.
    Stopped,
    /// The browser went away.
    ConnectionLost,
    /// An emergency stop was requested.
    EmergencyStop,
    /// A call to Moonraker failed.
    Failed {
        /// What went wrong.
        message: String,
    },
}

/// How a hold ended and whether the settings were put back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HoldEnded {
    /// Why it ended.
    pub reason: EndReason,
    /// How putting the settings back went.
    pub restore: Restore,
}

/// What a running hold sends.
#[derive(Debug, Clone, PartialEq)]
pub enum HoldEvent {
    /// The hold has the slot. Sent first.
    Started {
        /// The hold's id, which the emergency-stop endpoint takes.
        id: String,
    },
    /// The hold's state, about every 0.25 s.
    State(HoldState),
    /// A change was not applied.
    Rejected {
        /// What was wrong with the change.
        message: String,
    },
}

/// Why a hold may not start.
#[derive(Debug)]
pub enum Refusal {
    /// Failed pre-flight checks, all at once.
    Checks(Vec<CheckFailure>),
    /// A request the printer cannot take, before anything moves.
    Invalid(String),
}

/// What a hold needs from the daemon.
pub struct HoldDeps<'a> {
    /// The Moonraker supervisor.
    pub moonraker: &'a Moonraker,
    /// The one-test slot.
    pub slot: &'a TestSlot,
    /// The limits and timings.
    pub timings: HoldTimings,
}

/// A hold that passed pre-flight and owns the slot.
pub struct Hold {
    permit: SlotPermit,
    id: String,
    control: Rpc,
    timings: HoldTimings,
    plan: Plan,
}

struct Plan {
    drive: Drive,
    duration: f64,
    point: [f64; 3],
    /// The travel speed to the point, in mm/s.
    speed: f64,
    min: [f64; 3],
    max: [f64; 3],
    max_velocity: f64,
    saved: Saved,
}

fn failure(check: &str, message: impl Into<String>) -> Refusal {
    Refusal::Checks(vec![CheckFailure::new(check, message)])
}

fn triple(value: &Value) -> Option<[f64; 3]> {
    let list = value.as_array()?;
    Some([
        list.first()?.as_f64()?,
        list.get(1)?.as_f64()?,
        list.get(2)?.as_f64()?,
    ])
}

/// Checks the printer, takes the slot and resolves the hold.
///
/// # Errors
/// [`Refusal::Checks`] with every failed check, or [`Refusal::Invalid`] for a request the printer
/// cannot take.
pub async fn begin(deps: &HoldDeps<'_>, request: HoldRequest) -> Result<Hold, Refusal> {
    let quarantine = deps.slot.quarantine_remaining();
    let mut failures = check_printer(deps.moonraker, quarantine.is_some()).await;
    let permit = deps.slot.try_acquire();
    if permit.is_none() {
        let message = match deps
            .slot
            .current_id()
            .as_deref()
            .and_then(|id| id.strip_prefix("hold-"))
        {
            Some(since) => format!("a hold started {since} is running"),
            None => "another test is already running".to_owned(),
        };
        failures.push(CheckFailure::new("no_test_running", message));
    }
    if let Some(left) = quarantine {
        failures.push(quarantine_failure(left));
    }
    if !failures.is_empty() {
        return Err(Refusal::Checks(failures));
    }
    let permit =
        permit.ok_or_else(|| failure("no_test_running", "another test is already running"))?;
    let control = deps.moonraker.control().ok_or_else(|| {
        failure(
            "moonraker_connected",
            "the daemon is not connected to Moonraker",
        )
    })?;
    let unreadable = |message: String| failure("moonraker_connected", message);
    let settings = crate::moonraker::settings::read(&control)
        .await
        .map_err(&unreadable)?;
    let status = control
        .call(
            "printer.objects.query",
            Some(json!({"objects": {
                "toolhead": [
                    "position", "axis_minimum", "axis_maximum",
                    "max_velocity", "max_accel", "minimum_cruise_ratio",
                ],
                "input_shaper": null,
            }})),
            deps.timings.timeouts.query,
        )
        .await
        .map_err(|e| unreadable(format!("cannot read the toolhead: {e}")))?;
    let status = status.get("status").cloned().unwrap_or(Value::Null);
    let toolhead = status.get("toolhead").unwrap_or(&Value::Null);
    let position = toolhead
        .get("position")
        .and_then(triple)
        .ok_or_else(|| unreadable("the toolhead position is not reported".to_owned()))?;
    let min = toolhead.get("axis_minimum").and_then(triple);
    let max = toolhead.get("axis_maximum").and_then(triple);
    let (Some(min), Some(max)) = (min, max) else {
        return Err(unreadable(
            "the toolhead's travel limits are not reported".to_owned(),
        ));
    };
    let saved = Saved::from_status(&status).map_err(&unreadable)?;
    let accel_per_hz = request
        .accel_per_hz
        .or(settings.settings.accel_per_hz.value)
        .ok_or_else(|| {
            Refusal::Invalid(
                "the printer's acceleration per Hz cannot be read; enter one".to_owned(),
            )
        })?;
    let drive = Drive {
        axis: request.axis,
        frequency: request.frequency,
        accel_per_hz,
    };
    drive.validate().map_err(Refusal::Invalid)?;
    drive
        .check_velocity(saved.max_velocity)
        .map_err(Refusal::Invalid)?;
    let point = request
        .start_point
        .or(settings.default_point)
        .unwrap_or(position);
    let speed = settings
        .settings
        .move_speed
        .value
        .unwrap_or(DEFAULT_TRAVEL_SPEED);
    drive
        .check_room(point, min, max)
        .map_err(Refusal::Invalid)?;
    let duration = match request.duration_seconds {
        None => deps.timings.until_stopped_cap_seconds,
        Some(d) if d.is_finite() && d > 0.0 => d.min(deps.timings.until_stopped_cap_seconds),
        Some(d) => {
            return Err(Refusal::Invalid(format!(
                "the duration must be positive, not {d}"
            )));
        }
    };
    let id = format!("hold-{}", iso_utc(OffsetDateTime::now_utc()));
    permit.begin(&id);
    Ok(Hold {
        permit,
        id,
        control,
        timings: deps.timings.clone(),
        plan: Plan {
            drive,
            duration,
            point,
            speed,
            min,
            max,
            max_velocity: saved.max_velocity,
            saved,
        },
    })
}

/// What changes while a hold runs.
struct Live {
    /// Where the hold runs: the start point, then the verified position after travel.
    point: [f64; 3],
    target: Drive,
    working: f64,
    accel_for: f64,
    last_ramp: f64,
    last_state: f64,
}

enum Sent {
    Done(Result<(), String>),
    Interrupted(EndReason),
}

impl Hold {
    /// Runs the hold to its end and puts the settings back. The slot is freed when this returns.
    pub async fn run(
        self,
        mut commands: UnboundedReceiver<Command>,
        events: UnboundedSender<HoldEvent>,
    ) -> HoldEnded {
        let _ = events.send(HoldEvent::Started {
            id: self.id.clone(),
        });
        let start = Instant::now();
        let mut applied = false;
        let mut live = Live {
            point: self.plan.point,
            target: self.plan.drive,
            working: self.plan.drive.frequency,
            accel_for: self.plan.drive.frequency,
            last_ramp: 0.0,
            last_state: f64::NEG_INFINITY,
        };
        let reason = self
            .hold(&mut commands, &events, start, &mut applied, &mut live)
            .await;
        let drive = Drive {
            frequency: live.working,
            ..live.target
        };
        let _ = events.send(HoldEvent::State(HoldState {
            phase: HoldPhase::Stopping,
            elapsed_seconds: start.elapsed().as_secs_f64(),
            frequency: drive.frequency,
            axis: drive.axis,
            acceleration: drive.acceleration(),
            accel_limit: Drive {
                frequency: live.accel_for,
                ..drive
            }
            .acceleration(),
        }));
        let restore = if !applied {
            Restore::Done
        } else if reason == EndReason::EmergencyStop {
            Restore::NotNeeded
        } else {
            match self
                .call(&restore_script(&self.plan.saved), self.timings.call_timeout)
                .await
            {
                Ok(()) => Restore::Done,
                Err(e) => Restore::failed(&self.plan.saved, &e),
            }
        };
        HoldEnded { reason, restore }
    }

    async fn call(&self, script: &str, timeout: Duration) -> Result<(), String> {
        self.control
            .call(
                "printer.gcode.script",
                Some(json!({"script": script})),
                timeout,
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn check(&self, drive: &Drive, point: [f64; 3]) -> Result<(), String> {
        drive.validate()?;
        drive.check_velocity(self.plan.max_velocity)?;
        drive.check_room(point, self.plan.min, self.plan.max)
    }

    /// Applies one command. Returns the reason when it ends the hold.
    fn apply(
        &self,
        command: Option<Command>,
        live: &mut Live,
        events: &UnboundedSender<HoldEvent>,
    ) -> Option<EndReason> {
        let rejected = |message: String| {
            let _ = events.send(HoldEvent::Rejected { message });
        };
        match command {
            None | Some(Command::ConnectionLost) => Some(EndReason::ConnectionLost),
            Some(Command::Stop) => Some(EndReason::Stopped),
            Some(Command::SetFrequency(frequency)) => {
                let drive = Drive {
                    frequency,
                    ..live.target
                };
                match self.check(&drive, live.point) {
                    Ok(()) => live.target = drive,
                    Err(message) => rejected(message),
                }
                None
            }
            Some(Command::SetAxis(axis)) => {
                let drive = Drive {
                    axis,
                    ..live.target
                };
                match self.check(&drive, live.point) {
                    Ok(()) => live.target = drive,
                    Err(message) => rejected(message),
                }
                None
            }
        }
    }

    /// Sends a script while still handling commands. On a terminal command no more is queued,
    /// the call in flight is awaited for at most `in_flight_wait`, and the reason is returned.
    async fn send(
        &self,
        script: String,
        commands: &mut UnboundedReceiver<Command>,
        live: &mut Live,
        events: &UnboundedSender<HoldEvent>,
    ) -> Sent {
        let call = self.call(&script, self.timings.call_timeout);
        tokio::pin!(call);
        let mut tick = tokio::time::interval(STOP_POLL);
        loop {
            tokio::select! {
                result = &mut call => return Sent::Done(result),
                command = commands.recv() => {
                    if let Some(reason) = self.apply(command, live, events) {
                        let _ = tokio::time::timeout(self.timings.in_flight_wait, &mut call).await;
                        return Sent::Interrupted(reason);
                    }
                }
                _ = tick.tick() => {
                    if self.permit.stop_requested() {
                        let _ = tokio::time::timeout(self.timings.in_flight_wait, &mut call).await;
                        return Sent::Interrupted(EndReason::EmergencyStop);
                    }
                }
            }
        }
    }

    /// Waits `wait` while still handling commands and the emergency-stop flag.
    async fn pause(
        &self,
        wait: Duration,
        commands: &mut UnboundedReceiver<Command>,
        live: &mut Live,
        events: &UnboundedSender<HoldEvent>,
    ) -> Option<EndReason> {
        let deadline = tokio::time::Instant::now() + wait;
        let mut tick = tokio::time::interval(STOP_POLL);
        loop {
            tokio::select! {
                () = tokio::time::sleep_until(deadline) => return None,
                command = commands.recv() => {
                    if let Some(reason) = self.apply(command, live, events) {
                        return Some(reason);
                    }
                }
                _ = tick.tick() => {
                    if self.permit.stop_requested() {
                        return Some(EndReason::EmergencyStop);
                    }
                }
            }
        }
    }

    /// Sends a script like [`Self::send`], as a result.
    async fn send_checked(
        &self,
        script: String,
        commands: &mut UnboundedReceiver<Command>,
        live: &mut Live,
        events: &UnboundedSender<HoldEvent>,
    ) -> Result<(), EndReason> {
        match self.send(script, commands, live, events).await {
            Sent::Done(Ok(())) => Ok(()),
            Sent::Done(Err(message)) => Err(EndReason::Failed { message }),
            Sent::Interrupted(reason) => Err(reason),
        }
    }

    /// Waits until the toolhead has played everything queued, still handling commands.
    async fn settle(
        &self,
        commands: &mut UnboundedReceiver<Command>,
        live: &mut Live,
        events: &UnboundedSender<HoldEvent>,
        start: Instant,
    ) -> Result<(), EndReason> {
        let began = Instant::now();
        loop {
            let (print_time, estimated, _) = self
                .print_times(start)
                .await
                .map_err(|message| EndReason::Failed { message })?;
            if estimated >= print_time {
                return Ok(());
            }
            if began.elapsed().as_secs_f64() > TRAVEL_SETTLE_SECONDS {
                return Err(EndReason::Failed {
                    message: "the toolhead did not finish the travel in time".to_owned(),
                });
            }
            if let Some(reason) = self
                .pause(Duration::from_millis(250), commands, live, events)
                .await
            {
                return Err(reason);
            }
        }
    }

    /// Moves to the start point in paced segments, waits for the motion to finish, and
    /// corrects once if the toolhead missed. Afterwards the live point is where the toolhead is.
    async fn travel(
        &self,
        commands: &mut UnboundedReceiver<Command>,
        live: &mut Live,
        events: &UnboundedSender<HoldEvent>,
        start: Instant,
    ) -> Result<(), EndReason> {
        let plan = &self.plan;
        let read = |e: crate::capture::position::PositionError| EndReason::Failed {
            message: e.to_string(),
        };
        let here = position(&self.control, self.timings.timeouts.query)
            .await
            .map_err(read)?;
        let near = |a: [f64; 3]| (0..3).all(|i| (a[i] - plan.point[i]).abs() <= TOLERANCE_MM);
        if near(here) {
            live.point = here;
            return Ok(());
        }
        let mut pacer = Pacer::new(TRAVEL_BUFFER);
        let mut last = here;
        for target in segments(here, plan.point, plan.speed, self.timings.piece_seconds) {
            let distance = ((target[0] - last[0]).powi(2)
                + (target[1] - last[1]).powi(2)
                + (target[2] - last[2]).powi(2))
            .sqrt();
            let duration = distance / plan.speed;
            loop {
                let now = start.elapsed().as_secs_f64();
                let due = pacer.next_send_time(now, duration);
                if due <= now + 1e-4 {
                    pacer.sent(now, duration);
                    break;
                }
                let wait = Duration::from_secs_f64((due - now).min(STATE_INTERVAL));
                if let Some(reason) = self.pause(wait, commands, live, events).await {
                    return Err(reason);
                }
            }
            let _ = events.send(HoldEvent::State(HoldState {
                phase: HoldPhase::Travelling,
                elapsed_seconds: start.elapsed().as_secs_f64(),
                frequency: live.working,
                axis: live.target.axis,
                acceleration: live.target.acceleration(),
                accel_limit: live.target.acceleration(),
            }));
            let [x, y, z] = target;
            self.send_checked(
                format!("G90\nG1 X{x:.4} Y{y:.4} Z{z:.4} F{:.0}", plan.speed * 60.0),
                commands,
                live,
                events,
            )
            .await?;
            last = target;
        }
        self.settle(commands, live, events, start).await?;
        let mut got = position(&self.control, self.timings.timeouts.query)
            .await
            .map_err(read)?;
        if !near(got) {
            let target = [
                plan.point[0] - (got[0] - plan.point[0]),
                plan.point[1] - (got[1] - plan.point[1]),
                plan.point[2] - (got[2] - plan.point[2]),
            ];
            let [x, y, z] = target;
            self.send_checked(
                format!("G90\nG1 X{x:.4} Y{y:.4} Z{z:.4} F{:.0}", plan.speed * 60.0),
                commands,
                live,
                events,
            )
            .await?;
            self.settle(commands, live, events, start).await?;
            got = position(&self.control, self.timings.timeouts.query)
                .await
                .map_err(read)?;
            if !near(got) {
                return Err(EndReason::Failed {
                    message: format!(
                        "the toolhead could not reach the start point {:?}; it is at {got:?}",
                        plan.point
                    ),
                });
            }
        }
        live.point = got;
        Ok(())
    }

    /// `print_time` and `estimated_print_time`, and when the query was answered, in seconds on
    /// the hold's clock (the midpoint of the call).
    async fn print_times(&self, start: Instant) -> Result<(f64, f64, f64), String> {
        let before = Instant::now();
        let answer = self
            .control
            .call(
                "printer.objects.query",
                Some(json!({"objects": {"toolhead": ["print_time", "estimated_print_time"]}})),
                self.timings.timeouts.query,
            )
            .await
            .map_err(|e| e.to_string())?;
        let after = Instant::now();
        let toolhead = answer.get("status").and_then(|s| s.get("toolhead"));
        let read = |key: &str| toolhead.and_then(|t| t.get(key)).and_then(Value::as_f64);
        let (Some(print_time), Some(estimated)) =
            (read("print_time"), read("estimated_print_time"))
        else {
            return Err("the toolhead's print time is not reported".to_owned());
        };
        let mid = before + after.duration_since(before) / 2;
        Ok((
            print_time,
            estimated,
            mid.duration_since(start).as_secs_f64(),
        ))
    }

    #[allow(clippy::too_many_lines)]
    async fn hold(
        &self,
        commands: &mut UnboundedReceiver<Command>,
        events: &UnboundedSender<HoldEvent>,
        start: Instant,
        applied: &mut bool,
        live: &mut Live,
    ) -> EndReason {
        let plan = &self.plan;
        macro_rules! try_send {
            ($script:expr) => {
                match self.send($script, commands, live, events).await {
                    Sent::Done(Ok(())) => {}
                    Sent::Done(Err(message)) => return EndReason::Failed { message },
                    Sent::Interrupted(reason) => return reason,
                }
            };
        }
        try_send!(format!("SAVE_GCODE_STATE NAME={STATE_NAME}"));
        *applied = true;
        try_send!(setup_script(&plan.saved, &plan.drive));
        if let Err(reason) = self.travel(commands, live, events, start).await {
            return reason;
        }

        // Klipper starts motion on an idle toolhead after its own buffer. Measure it with a
        // move of 0.01 mm that returns to its start, then read Klipper's print time after the
        // lookahead queue has flushed.
        let (_, estimated0, mid0) = match self.print_times(start).await {
            Ok(v) => v,
            Err(message) => return EndReason::Failed { message },
        };
        let off = estimated0 - mid0;
        let t_cal = start.elapsed().as_secs_f64();
        try_send!(format!(
            "; start buffer\nG91\nG1 X{CALIBRATION_MM:.4} F600\nG1 X-{CALIBRATION_MM:.4} F600"
        ));
        if let Some(reason) = self.pause(CALIBRATION_WAIT, commands, live, events).await {
            return reason;
        }
        let (print_time, ..) = match self.print_times(start).await {
            Ok(v) => v,
            Err(message) => return EndReason::Failed { message },
        };
        let cal_duration = 4.0 * (CALIBRATION_MM / plan.drive.acceleration()).sqrt();
        let buffer = (print_time - (t_cal + off) - cal_duration).clamp(0.0, 2.0);

        let mut pacer = Pacer::new(buffer);
        let mut first: Option<f64> = None;
        loop {
            let now = start.elapsed().as_secs_f64();
            if first.is_some_and(|f| now - f >= plan.duration) {
                return EndReason::TimeLimit;
            }
            if self.permit.stop_requested() {
                return EndReason::EmergencyStop;
            }
            live.working = ramp(live.working, live.target.frequency, now - live.last_ramp);
            live.last_ramp = now;
            let drive = Drive {
                frequency: live.working,
                ..live.target
            };
            if now - live.last_state >= STATE_INTERVAL {
                live.last_state = now;
                let _ = events.send(HoldEvent::State(HoldState {
                    phase: HoldPhase::Holding,
                    elapsed_seconds: now,
                    frequency: drive.frequency,
                    axis: drive.axis,
                    acceleration: drive.acceleration(),
                    accel_limit: Drive {
                        frequency: live.accel_for,
                        ..drive
                    }
                    .acceleration(),
                }));
            }
            let pairs = pairs_for(self.timings.piece_seconds, drive.frequency);
            let duration = piece_duration(pairs, drive.frequency);
            let due = pacer.next_send_time(now, duration);
            if due > now + 1e-4 {
                let wait = Duration::from_secs_f64((due - now).min(STATE_INTERVAL));
                if let Some(reason) = self.pause(wait, commands, live, events).await {
                    return reason;
                }
                continue;
            }
            let mut script = String::new();
            if (drive.frequency - live.accel_for).abs() > 1e-9 {
                script.push_str(&accel_line(&drive));
                script.push('\n');
                live.accel_for = drive.frequency;
            }
            script.push_str(&piece_script(&drive, pairs));
            pacer.sent(now, duration);
            first.get_or_insert(now);
            try_send!(script);
        }
    }
}
