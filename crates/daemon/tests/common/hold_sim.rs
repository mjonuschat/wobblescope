//! A simulated printer for held excitations: `printer.gcode.script` moves a simulated toolhead
//! in relative or absolute mode, tracks the limits and input shaping a hold changes, and keeps a
//! print-time queue like Klipper's, with an idle start buffer.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use wobblescope::capture::slot::TestSlot;
use wobblescope::config::Settings;
use wobblescope::moonraker::{Moonraker, spawn};

use super::fake_moonraker::{FakeMoonraker, Script, eventually};
use super::fake_printer::{FakePrinter, fixture};
use super::sim_printer::{DEADLINE, fast};

/// The print time the simulation starts at.
const EPOCH: f64 = 100.0;
/// Klipper's idle start buffer as the simulation applies it, in seconds.
pub const START_BUFFER: f64 = 0.37;

type Failure = (Box<dyn Fn(&str, usize) -> bool + Send>, &'static str);

struct State {
    started: Instant,
    scripts: Vec<(f64, String)>,
    absolute: bool,
    position: [f64; 3],
    max_accel: f64,
    print_time: f64,
    pieces: usize,
    failures: Vec<Failure>,
    delays: Vec<(usize, Duration)>,
    /// Added to the target of every absolute move, as a G-code offset or a mesh would.
    landing_error: [f64; 3],
}

/// The simulated printer, the supervisor and the slot. Clones of the inner state are shared.
pub struct HoldSim {
    pub fake: Arc<FakeMoonraker>,
    pub printer: FakePrinter,
    pub moonraker: Moonraker,
    pub slot: TestSlot,
    state: Arc<Mutex<State>>,
}

fn word(line: &str, key: &str) -> Option<f64> {
    line.split_whitespace()
        .find_map(|w| w.strip_prefix(key).and_then(|v| v.parse::<f64>().ok()))
}

fn text_word<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace().find_map(|w| w.strip_prefix(key))
}

impl HoldSim {
    pub async fn start() -> Self {
        let fake = Arc::new(FakeMoonraker::start().await);
        let printer = FakePrinter::new(
            &fake,
            "ready",
            json!({"adxl345": {"cs_pin": "PA4"}, "resonance_tester": {"accel_chip": "adxl345"}}),
        );
        let real = fixture("real_settings.json");
        printer.set_settings(real["status"]["configfile"]["settings"].clone());
        printer.set_field("toolhead", "position", json!([150.5, 154.0, 20.0, 0.0]));
        printer.set_field("toolhead", "max_velocity", json!(500.0));
        printer.set_field("toolhead", "max_accel", json!(3000.0));
        printer.set_field("toolhead", "minimum_cruise_ratio", json!(0.5));
        for (field, value) in [
            ("shaper_type_x", json!("mzv")),
            ("shaper_freq_x", json!(51.2)),
            ("damping_ratio_x", json!(0.1)),
            ("shaper_type_y", json!("mzv")),
            ("shaper_freq_y", json!(44.0)),
            ("damping_ratio_y", json!(0.1)),
        ] {
            printer.set_field("input_shaper", field, value);
        }
        let state = Arc::new(Mutex::new(State {
            started: Instant::now(),
            scripts: vec![],
            absolute: true,
            position: [150.5, 154.0, 20.0],
            max_accel: 3000.0,
            print_time: EPOCH,
            pieces: 0,
            failures: vec![],
            delays: vec![],
            landing_error: [0.0; 3],
        }));
        let sim_state = state.clone();
        let sim_printer = printer.clone();
        fake.on_fn("printer.gcode.script", move |params, _| {
            script(
                &sim_state,
                &sim_printer,
                params["script"].as_str().unwrap_or_default(),
            )
        });
        let clock_state = state.clone();
        let clock_printer = printer.clone();
        tokio::spawn(async move {
            loop {
                {
                    let s = clock_state.lock().unwrap();
                    let est = EPOCH + s.started.elapsed().as_secs_f64();
                    // The toolhead is at its target once the queued motion has played. Set it
                    // before the clock, so a query that finds the queue idle also finds it there.
                    if est >= s.print_time {
                        let p = s.position;
                        clock_printer.set_field(
                            "toolhead",
                            "position",
                            json!([p[0], p[1], p[2], 0.0]),
                        );
                    }
                    clock_printer.set_field("toolhead", "estimated_print_time", json!(est));
                    clock_printer.set_field("toolhead", "print_time", json!(s.print_time));
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
        let settings = Settings {
            moonraker_url: fake.http_url().parse().unwrap(),
            ..Settings::default()
        };
        let (moonraker, _) = spawn(&settings, fast(), false);
        eventually("a connection", DEADLINE, || {
            moonraker.chips().current && moonraker.control().is_some()
        })
        .await;
        Self {
            fake,
            printer,
            moonraker,
            slot: TestSlot::default(),
            state,
        }
    }

    /// Seconds since the simulation started, on the clock the recorded scripts use.
    pub fn elapsed(&self) -> f64 {
        self.state.lock().unwrap().started.elapsed().as_secs_f64()
    }

    /// Every script received, in order.
    pub fn scripts(&self) -> Vec<String> {
        let s = self.state.lock().unwrap();
        s.scripts.iter().map(|(_, text)| text.clone()).collect()
    }

    /// Every script received with the time it arrived.
    pub fn timed_scripts(&self) -> Vec<(f64, String)> {
        self.state.lock().unwrap().scripts.clone()
    }

    /// The pieces received so far.
    pub fn pieces(&self) -> usize {
        self.state.lock().unwrap().pieces
    }

    /// The `G1` lines in the pieces received.
    pub fn piece_moves(&self) -> usize {
        self.scripts()
            .iter()
            .filter(|s| s.starts_with("G91\nG1"))
            .map(|s| s.lines().filter(|l| l.starts_with("G1 ")).count())
            .sum()
    }

    pub fn field(&self, object: &str, field: &str) -> Value {
        self.printer.field(object, field)
    }

    /// Where the simulated toolhead is.
    pub fn position(&self) -> [f64; 3] {
        self.state.lock().unwrap().position
    }

    pub fn set_position(&self, position: [f64; 3]) {
        self.state.lock().unwrap().position = position;
        self.printer.set_field(
            "toolhead",
            "position",
            json!([position[0], position[1], position[2], 0.0]),
        );
    }

    /// Fails every script for which `when(script, piece_number)` is true with `message`. The
    /// failing script changes nothing, as in Klipper.
    pub fn fail_when(
        &self,
        when: impl Fn(&str, usize) -> bool + Send + 'static,
        message: &'static str,
    ) {
        self.state
            .lock()
            .unwrap()
            .failures
            .push((Box::new(when), message));
    }

    /// Delays the reply to the nth piece (from 1). The piece itself takes effect at once.
    pub fn delay_piece(&self, nth: usize, delay: Duration) {
        self.state.lock().unwrap().delays.push((nth, delay));
    }

    /// Makes every absolute move land `error` mm away from its target.
    pub fn set_landing_error(&self, error: [f64; 3]) {
        self.state.lock().unwrap().landing_error = error;
    }

    /// The absolute travel moves received (`G90` then `G1`), in order.
    pub fn travel_scripts(&self) -> Vec<String> {
        self.scripts()
            .into_iter()
            .filter(|s| s.starts_with("G90\nG1"))
            .collect()
    }

    pub fn set_print_state(&self, state: &str) {
        self.printer.set_field("print_stats", "state", json!(state));
    }

    pub fn set_homed(&self, axes: &str) {
        self.printer
            .set_field("toolhead", "homed_axes", json!(axes));
    }
}

fn script(state: &Arc<Mutex<State>>, printer: &FakePrinter, text: &str) -> Script {
    let mut s = state.lock().unwrap();
    let elapsed = s.started.elapsed().as_secs_f64();
    let is_piece = text.starts_with("G91\nG1");
    if is_piece {
        s.pieces += 1;
    }
    let piece_no = s.pieces;
    s.scripts.push((elapsed, text.to_owned()));
    for (when, message) in &s.failures {
        if when(text, piece_no) {
            return Script::error(400, message);
        }
    }
    for line in text.lines() {
        if line == "G91" {
            s.absolute = false;
        } else if line == "G90" || line.starts_with("RESTORE_GCODE_STATE") {
            s.absolute = true;
        } else if line.starts_with("SET_VELOCITY_LIMIT") {
            if let Some(v) = word(line, "VELOCITY=") {
                printer.set_field("toolhead", "max_velocity", json!(v));
            }
            if let Some(a) = word(line, "ACCEL=") {
                s.max_accel = a;
                printer.set_field("toolhead", "max_accel", json!(a));
            }
            if let Some(r) = word(line, "MINIMUM_CRUISE_RATIO=") {
                printer.set_field("toolhead", "minimum_cruise_ratio", json!(r));
            }
        } else if line.starts_with("SET_INPUT_SHAPER") {
            for (key, field) in [
                ("SHAPER_FREQ_X=", "shaper_freq_x"),
                ("SHAPER_FREQ_Y=", "shaper_freq_y"),
                ("DAMPING_RATIO_X=", "damping_ratio_x"),
                ("DAMPING_RATIO_Y=", "damping_ratio_y"),
            ] {
                if let Some(v) = word(line, key) {
                    printer.set_field("input_shaper", field, json!(v));
                }
            }
            for (key, field) in [
                ("SHAPER_TYPE_X=", "shaper_type_x"),
                ("SHAPER_TYPE_Y=", "shaper_type_y"),
            ] {
                if let Some(v) = text_word(line, key) {
                    printer.set_field("input_shaper", field, json!(v));
                }
            }
        } else if line.starts_with("G1 ") {
            let before = s.position;
            let (dx, dy, dz) = (
                word(line, "X").unwrap_or(f64::NAN),
                word(line, "Y").unwrap_or(f64::NAN),
                word(line, "Z").unwrap_or(f64::NAN),
            );
            let landing = s.landing_error;
            let target = if s.absolute {
                [
                    if dx.is_nan() {
                        before[0]
                    } else {
                        dx + landing[0]
                    },
                    if dy.is_nan() {
                        before[1]
                    } else {
                        dy + landing[1]
                    },
                    if dz.is_nan() {
                        before[2]
                    } else {
                        dz + landing[2]
                    },
                ]
            } else {
                [
                    before[0] + if dx.is_nan() { 0.0 } else { dx },
                    before[1] + if dy.is_nan() { 0.0 } else { dy },
                    before[2] + if dz.is_nan() { 0.0 } else { dz },
                ]
            };
            let dist = ((target[0] - before[0]).powi(2)
                + (target[1] - before[1]).powi(2)
                + (target[2] - before[2]).powi(2))
            .sqrt();
            let duration = if s.absolute {
                dist / (word(line, "F").unwrap_or(3000.0) / 60.0)
            } else {
                2.0 * (dist / s.max_accel).sqrt()
            };
            let est = EPOCH + elapsed;
            let start = if s.print_time > est {
                s.print_time
            } else {
                est + START_BUFFER
            };
            s.print_time = start + duration;
            s.position = target;
        }
    }
    printer.set_field("toolhead", "print_time", json!(s.print_time));
    let delay = if is_piece {
        s.delays
            .iter()
            .find(|(n, _)| *n == piece_no)
            .map(|(_, d)| *d)
    } else {
        None
    };
    drop(s);
    match delay {
        Some(d) => Script::after(d, Script::Result(json!("ok"))),
        None => Script::Result(json!("ok")),
    }
}
