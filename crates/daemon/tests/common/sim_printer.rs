//! A simulated printer for the runner: the fake Moonraker's `printer.gcode.script` moves a
//! simulated toolhead and advances a print-time clock, and the fake bridge's chips deliver
//! samples stamped with that clock, so a whole test runs in milliseconds.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;
use time::OffsetDateTime;
use wobblescope::capture::live::{FinishedOutcome, LiveHub, LiveSnapshot};
use wobblescope::capture::preflight::{Deps, Refusal, run as preflight};
use wobblescope::capture::request::TestPlan;
use wobblescope::capture::runner::{Job, Runner, RunnerTimings};
use wobblescope::capture::slot::TestSlot;
use wobblescope::config::Settings;
use wobblescope::moonraker::tls::Tls;
use wobblescope::moonraker::{Moonraker, Timings, spawn};
use wobblescope::startup::StorageCell;
use wobblescope::storage::Storage;

use super::fake_bridge::FakeBridge;
use super::fake_moonraker::{FakeMoonraker, Gate, Script, eventually};
use super::fake_printer::FakePrinter;

pub const DEADLINE: Duration = Duration::from_secs(10);

/// The print time the simulation starts at, and the first sample of every chip.
const START_T: f64 = 100.0;

/// The two chips of the simulated printer: config section, sensor and rate.
pub const CHIPS: [(&str, &str, f64); 2] = [
    ("adxl345", "adxl345", 500.0),
    ("lis2dw ebb36", "ebb36", 400.0),
];

/// How the move to a test point behaves.
#[derive(Clone, Copy, PartialEq)]
pub enum Move {
    /// Lands on the commanded point (plus the offset).
    Normal,
    /// Never answers.
    Silent,
    /// Answers but the toolhead stays where it was.
    Frozen,
    /// Waits for [`Sim::release_move`].
    Pending,
}

/// When a silencing rule takes effect.
#[derive(Clone, Copy)]
pub enum When {
    /// At the first move once this many sweeps have started.
    MoveAfterSweeps(usize),
    /// When the nth sweep (from 0) starts.
    SweepStart(usize),
}

/// How one sweep call behaves.
#[derive(Clone)]
pub enum Sweep {
    /// Delivers its samples and returns.
    Normal,
    /// Delivers its first samples and then waits for [`Sim::release_pending`].
    Pending,
    /// Waits for [`Sim::release_pending`] without delivering anything.
    PendingQuiet,
    /// Answers with a G-code error.
    Error(String),
    /// Never answers.
    Silent,
}

struct State {
    /// The print-time clock.
    t: f64,
    next_sample_at: HashMap<String, f64>,
    silent: Vec<String>,
    sweep_seconds: f64,
    offset: [f64; 3],
    behaviors: HashMap<usize, Sweep>,
    sweeps: Vec<String>,
    scripts: Vec<String>,
    pending: Option<Gate>,
    /// How long the toolhead stays `Printing` after a sweep.
    idle_delay: Duration,
    counters: HashMap<String, (u64, u64)>,
    movement: Move,
    pending_move: Option<Gate>,
    rules: Vec<(Option<String>, When)>,
    hooks: HashMap<usize, Arc<dyn Fn() + Send + Sync>>,
}

/// The simulated printer. Clones share it.
#[derive(Clone)]
pub struct Sim {
    state: Arc<Mutex<State>>,
    /// True while the pre-flight feeder may push; the first script turns it off.
    feeding: Arc<Mutex<bool>>,
    pub fake: Arc<FakeMoonraker>,
    pub printer: FakePrinter,
}

fn config() -> Value {
    json!({
        "adxl345": {"cs_pin": "PA4"},
        "lis2dw ebb36": {"cs_pin": "ebb36: PA4", "axes_map": "x, z, -y"},
        "resonance_tester": {"accel_chip": "adxl345"},
    })
}

fn parse_g1(script: &str) -> Option<[f64; 3]> {
    let line = script.lines().find(|l| l.starts_with("G1 "))?;
    let mut at = [0.0; 3];
    for word in line.split_whitespace() {
        let value = word.get(1..).and_then(|v| v.parse::<f64>().ok());
        match (word.as_bytes()[0], value) {
            (b'X', Some(v)) => at[0] = v,
            (b'Y', Some(v)) => at[1] = v,
            (b'Z', Some(v)) => at[2] = v,
            _ => {}
        }
    }
    Some(at)
}

impl Sim {
    pub async fn start() -> Self {
        Self::start_with_config(config()).await
    }

    /// Like [`Sim::start`], with `config` as the printer's `configfile.config`.
    pub async fn start_with_config(config: Value) -> Self {
        let fake = Arc::new(FakeMoonraker::start().await);
        let printer = FakePrinter::new(&fake, "ready", config);
        printer.set_field("mcu ebb36", "mcu_version", json!("v0.13.0-820-g0cee841e"));
        printer.set_field("toolhead", "position", json!([0.0, 0.0, 0.0, 0.0]));
        let real = super::fake_printer::fixture("real_settings.json");
        printer.set_settings(real["status"]["configfile"]["settings"].clone());
        let state = Arc::new(Mutex::new(State {
            t: START_T,
            next_sample_at: HashMap::new(),
            silent: vec![],
            sweep_seconds: 2.0,
            offset: [0.0; 3],
            behaviors: HashMap::new(),
            sweeps: vec![],
            scripts: vec![],
            pending: None,
            idle_delay: Duration::ZERO,
            counters: HashMap::new(),
            movement: Move::Normal,
            pending_move: None,
            rules: vec![],
            hooks: HashMap::new(),
        }));
        let sim = Self {
            state,
            feeding: Arc::new(Mutex::new(false)),
            fake,
            printer,
        };
        let handler = sim.clone();
        sim.fake.on_fn("printer.gcode.script", move |params, _| {
            handler.script(params["script"].as_str().unwrap_or_default())
        });
        sim
    }

    pub fn bridge(&self) -> &FakeBridge {
        self.fake.bridge()
    }

    // ---- scripting ----

    pub fn set_behavior(&self, nth: usize, behavior: Sweep) {
        self.state.lock().unwrap().behaviors.insert(nth, behavior);
    }

    pub fn set_offset(&self, offset: [f64; 3]) {
        self.state.lock().unwrap().offset = offset;
    }

    pub fn set_idle_delay(&self, delay: Duration) {
        self.state.lock().unwrap().idle_delay = delay;
    }

    pub fn set_counters(&self, sensor: &str, errors: u64, overflows: u64) {
        self.state
            .lock()
            .unwrap()
            .counters
            .insert(sensor.to_owned(), (errors, overflows));
    }

    pub fn set_move(&self, movement: Move) {
        self.state.lock().unwrap().movement = movement;
    }

    /// Stops `sensor` (every sensor for `None`) from delivering when `when` happens.
    pub fn silence_when(&self, sensor: Option<&str>, when: When) {
        self.state
            .lock()
            .unwrap()
            .rules
            .push((sensor.map(str::to_owned), when));
    }

    /// Runs `hook` when the nth sweep (from 0) has delivered its samples, before it answers.
    pub fn on_sweep_done(&self, nth: usize, hook: impl Fn() + Send + Sync + 'static) {
        self.state.lock().unwrap().hooks.insert(nth, Arc::new(hook));
    }

    /// Answers the pending move as a move that did not happen.
    pub fn release_move(&self) {
        let gate = self.state.lock().unwrap().pending_move.take();
        gate.expect("a move is pending")
            .release(Script::error(400, "Klipper state: Shutdown"));
    }

    /// Stops `sensor` from delivering.
    pub fn silence(&self, sensor: &str) {
        self.state.lock().unwrap().silent.push(sensor.to_owned());
    }

    /// The `TEST_RESONANCES` commands received, in order.
    pub fn sweeps(&self) -> Vec<String> {
        self.state.lock().unwrap().sweeps.clone()
    }

    /// Every script received, in order.
    pub fn scripts(&self) -> Vec<String> {
        self.state.lock().unwrap().scripts.clone()
    }

    pub fn g1_count(&self) -> usize {
        self.scripts().iter().filter(|s| s.contains("G1 ")).count()
    }

    /// Ends the pending sweep with its normal samples, or with an error.
    pub fn release_pending(&self, ok: bool) {
        let gate = {
            let mut s = self.state.lock().unwrap();
            let gate = s.pending.take().expect("a sweep is pending");
            if ok {
                let end = s.t + s.sweep_seconds;
                self.advance(&mut s, end + 0.3);
                s.t = end;
            }
            gate
        };
        if ok {
            self.sync_clock();
            self.printer
                .set_field("idle_timeout", "state", json!("Ready"));
            gate.release(Script::Result(json!("ok")));
        } else {
            gate.release(Script::error(400, "Klipper state: Shutdown"));
        }
    }

    // ---- the simulation ----

    fn sync_clock(&self) {
        let t = self.state.lock().unwrap().t;
        self.printer.set_field("toolhead", "print_time", json!(t));
        self.printer
            .set_field("toolhead", "estimated_print_time", json!(t));
    }

    /// Delivers samples up to `to` on every sensor that is not silenced, each continuing where
    /// its last batch ended.
    fn advance(&self, s: &mut State, to: f64) {
        for (_, sensor, rate) in CHIPS {
            if s.silent.iter().any(|x| x == sensor) {
                continue;
            }
            let counters = s.counters.get(sensor).copied().unwrap_or((0, 0));
            let step = 1.0 / rate;
            let mut at = s.next_sample_at.get(sensor).copied().unwrap_or(START_T);
            while at < to {
                let rows: Vec<[f64; 4]> = (0..50)
                    .map(|k| {
                        let t = at + f64::from(k) * step;
                        [
                            t,
                            (t * 40.0).sin() * 200.0,
                            (t * 31.0).cos() * 50.0,
                            9800.0 + (t * 17.0).sin() * 100.0,
                        ]
                    })
                    .collect();
                at += 50.0 * step;
                self.bridge().push(sensor, &rows, Some(counters));
            }
            // A batch can run past `to`; the next one continues after it, as a real chip does.
            s.next_sample_at.insert((*sensor).to_owned(), at);
        }
    }

    fn apply_rules(&self, s: &mut State, now: When) {
        let started = s.sweeps.len();
        let due: Vec<Option<String>> = s
            .rules
            .iter()
            .filter(|(_, when)| match (when, now) {
                (When::MoveAfterSweeps(n), When::MoveAfterSweeps(_)) => started >= *n,
                (When::SweepStart(n), When::SweepStart(_)) => started == *n + 1,
                _ => false,
            })
            .map(|(sensor, _)| sensor.clone())
            .collect();
        for sensor in due {
            match sensor {
                Some(sensor) => s.silent.push(sensor),
                None => s.silent.extend(CHIPS.iter().map(|c| c.1.to_owned())),
            }
        }
    }

    /// Gives every chip valid samples (timestamps below the clock) until the first script, so
    /// pre-flight can prove them without the feeder ever interleaving with the simulation.
    pub fn feed_until_motion(&self) {
        *self.feeding.lock().unwrap() = true;
        let sim = self.clone();
        tokio::spawn(async move {
            let mut k = 0u64;
            loop {
                {
                    let on = sim.feeding.lock().unwrap();
                    if !*on {
                        return;
                    }
                    for (_, sensor, rate) in CHIPS {
                        let rows: Vec<[f64; 4]> = (0..20)
                            .map(|i| [50.0 + (k * 20 + i) as f64 / rate, 0.0, 0.0, 9800.0])
                            .collect();
                        sim.bridge().push(sensor, &rows, Some((0, 0)));
                    }
                }
                k += 1;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
    }

    fn script(&self, script: &str) -> Script {
        *self.feeding.lock().unwrap() = false;
        let mut s = self.state.lock().unwrap();
        s.scripts.push(script.to_owned());
        if script.starts_with("TEST_RESONANCES") {
            let n = s.sweeps.len();
            s.sweeps
                .push(script.lines().next().unwrap_or_default().to_owned());
            self.apply_rules(&mut s, When::SweepStart(0));
            let behavior = s.behaviors.get(&n).cloned().unwrap_or(Sweep::Normal);
            self.printer
                .set_field("idle_timeout", "state", json!("Printing"));
            match behavior {
                Sweep::Error(message) => {
                    self.printer
                        .set_field("idle_timeout", "state", json!("Ready"));
                    return Script::Error(400, Box::leak(message.into_boxed_str()));
                }
                Sweep::Silent => {
                    // The chips keep streaming while the call never answers.
                    let to = s.t + 1.5;
                    self.advance(&mut s, to);
                    return Script::Silent;
                }
                Sweep::Pending | Sweep::PendingQuiet => {
                    if matches!(behavior, Sweep::Pending) {
                        // Past the window start, which follows the move end by the lead-in.
                        let to = s.t + 1.5;
                        self.advance(&mut s, to);
                    }
                    let gate = Gate::new();
                    s.pending = Some(gate.clone());
                    return Script::Gated(gate);
                }
                Sweep::Normal => {}
            }
            let end = s.t + s.sweep_seconds;
            // The chips keep streaming a little past the end of the move.
            self.advance(&mut s, end + 0.3);
            s.t = end;
            let delay = s.idle_delay;
            let hook = s.hooks.get(&n).cloned();
            drop(s);
            self.sync_clock();
            if let Some(hook) = hook {
                hook();
            }
            let printer = self.printer.clone();
            if delay.is_zero() {
                printer.set_field("idle_timeout", "state", json!("Ready"));
            } else {
                tokio::spawn(async move {
                    tokio::time::sleep(delay).await;
                    printer.set_field("idle_timeout", "state", json!("Ready"));
                });
            }
            return Script::Result(json!("ok"));
        }
        if let Some(at) = parse_g1(script) {
            self.apply_rules(&mut s, When::MoveAfterSweeps(0));
            match s.movement {
                Move::Silent => return Script::Silent,
                Move::Pending => {
                    let gate = Gate::new();
                    s.pending_move = Some(gate.clone());
                    return Script::Gated(gate);
                }
                Move::Normal => {
                    let o = s.offset;
                    self.printer.set_field(
                        "toolhead",
                        "position",
                        json!([at[0] + o[0], at[1] + o[1], at[2] + o[2], 0.0]),
                    );
                }
                Move::Frozen => {}
            }
            let to = s.t + 0.5;
            s.t = to;
            self.advance(&mut s, to + 0.3);
            drop(s);
            self.sync_clock();
        }
        Script::Result(json!("ok"))
    }
}

pub fn fast() -> Timings {
    let ms = Duration::from_millis;
    Timings {
        connect: ms(500),
        request: ms(1000),
        configfile: ms(1000),
        poll_ready: ms(100),
        poll_not_ready: ms(20),
        backoff_start: ms(10),
        backoff_max: ms(40),
    }
}

pub fn fast_runner() -> RunnerTimings {
    let ms = Duration::from_millis;
    RunnerTimings {
        poll: ms(20),
        drain: ms(500),
        idle_wait: ms(800),
        idle_poll: ms(20),
        shutdown_grace: ms(100),
        positioning: wobblescope::capture::position::Timeouts {
            script: ms(1000),
            query: ms(500),
        },
        sweep_timeout: Some(ms(1500)),
        uncertain_cap: Duration::from_secs(30),
    }
}

/// A whole rig: the simulated printer, the supervisor, storage, the slot and the live hub.
pub struct Rig {
    pub sim: Sim,
    pub moonraker: Moonraker,
    pub storage: Storage,
    pub cell: StorageCell,
    pub slot: TestSlot,
    pub hub: LiveHub,
    pub runner: Runner,
    pub dir: TempDir,
}

pub async fn rig() -> Rig {
    rig_with(fast_runner(), u64::MAX / 4).await
}

pub async fn rig_with(timings: RunnerTimings, cap: u64) -> Rig {
    rig_on(Sim::start().await, timings, cap).await
}

/// A rig whose printer also has the accelerometer sections `extra` (for example `adxl345 b`).
pub async fn rig_with_extra_chips(extra: &[&str]) -> Rig {
    let mut printer_config = config();
    for section in extra {
        printer_config[*section] = json!({"cs_pin": "PA4"});
    }
    let sim = Sim::start_with_config(printer_config).await;
    rig_on(sim, fast_runner(), u64::MAX / 4).await
}

async fn rig_on(sim: Sim, timings: RunnerTimings, cap: u64) -> Rig {
    let settings = Settings {
        moonraker_url: sim.fake.http_url().parse().unwrap(),
        ..Settings::default()
    };
    let (moonraker, _) = spawn(&settings, fast(), false, Tls::default());
    eventually("current chips", DEADLINE, || {
        moonraker.chips().current && moonraker.control().is_some()
    })
    .await;
    let dir = TempDir::new().unwrap();
    let (storage, _) = super::open(dir.path(), cap);
    let slot = TestSlot::default();
    let hub = LiveHub::new();
    let runner = Runner {
        moonraker: moonraker.clone(),
        storage: storage.clone(),
        slot: slot.clone(),
        hub: hub.clone(),
        max_segment_seconds: 600,
        timings,
    };
    Rig {
        cell: super::cell(Some(storage.clone())),
        sim,
        moonraker,
        storage,
        slot,
        hub,
        runner,
        dir,
    }
}

impl Rig {
    /// Runs pre-flight for `plan` and, when it passes, starts the runner.
    pub async fn start(&self, plan: &TestPlan) -> Result<tokio::task::JoinHandle<()>, Refusal> {
        let permit = self.slot.try_acquire().expect("the slot is free");
        let mut deps = Deps::new(&self.moonraker, &self.cell, &self.slot);
        deps.availability_timeout = Duration::from_secs(1);
        self.sim.feed_until_motion();
        let started = preflight(&deps, plan, false, OffsetDateTime::now_utc()).await?;
        Ok(self.runner.spawn(Job {
            started,
            plan: plan.clone(),
            permit,
        }))
    }

    /// Waits for the test to end and returns what the live state says about it.
    pub async fn finished(&self) -> wobblescope::capture::live::FinishedTest {
        eventually("the test to end", DEADLINE, || {
            let s = self.hub.snapshot();
            s.test.is_none() && s.last.is_some() && !self.slot.is_held()
        })
        .await;
        self.hub.snapshot().last.unwrap()
    }

    pub async fn until(&self, what: &str, condition: impl FnMut() -> bool) {
        eventually(what, DEADLINE, condition).await;
    }

    pub fn snapshot(&self) -> LiveSnapshot {
        self.hub.snapshot()
    }

    /// The sealed session's ids.
    pub fn sessions(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .storage
            .list(&wobblescope::storage::index::ListFilter::default())
            .into_iter()
            .map(|s| s.id)
            .collect();
        v.sort();
        v
    }

    /// Whether the fake bridge has a subscription open.
    pub fn bridges_open(&self) -> usize {
        self.sim.bridge().open_total()
    }

    pub fn leftovers(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(super::sessions_dir(self.dir.path()))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }
}

pub fn outcome(f: &wobblescope::capture::live::FinishedTest) -> FinishedOutcome {
    f.outcome
}

impl Rig {
    /// The API router sharing this rig's slot and live hub.
    pub fn app(&self) -> axum::Router {
        self.app_with_stream_write_timeout(Duration::from_secs(10))
    }

    /// The API router with a stream viewer's write timeout set to `timeout`.
    pub fn app_with_stream_write_timeout(&self, timeout: Duration) -> axum::Router {
        self.app_with_stream_timings(wobblescope::api::StreamTimings {
            write_timeout: timeout,
            ..wobblescope::api::StreamTimings::default()
        })
    }

    /// The API router with the stream viewer's write and liveness limits set to `timings`.
    pub fn app_with_stream_timings(
        &self,
        timings: wobblescope::api::StreamTimings,
    ) -> axum::Router {
        wobblescope::api::mount_capture(
            self.cell.clone(),
            self.moonraker.clone(),
            wobblescope::api::ImportGate::default(),
            wobblescope::api::CaptureState {
                slot: self.slot.clone(),
                hub: self.hub.clone(),
                max_segment_seconds: 600,
                timings: self.runner.timings,
                stream_timings: timings,
                hold: wobblescope::api::HoldSocketTimings::default(),
            },
        )
    }
}

/// Serves `app` on a local port and returns its address.
pub async fn serve(app: axum::Router) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}
