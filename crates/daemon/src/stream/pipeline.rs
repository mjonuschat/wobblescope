//! One chip's samples, filtered and batched into frames for one viewer.
//!
//! The pipeline does no I/O and never sleeps: all time comes in through the `now` argument.

use std::time::{Duration, Instant};

use wobblescope_core::decimate::Decimator;
use wobblescope_core::samples::RawSamples;

use super::protocol::{ServerMessage, StreamLimits, clamp_request, encode_frame};
use crate::capture::availability::rate_from_times;
use crate::moonraker::bridge::Batch;
use crate::moonraker::chips::Chip;

/// How much timestamp time is measured before the filter is built, in seconds.
const MEASURE_SECONDS: f64 = 0.3;
/// How often health is sent and the measured rate is re-checked.
const HEALTH_INTERVAL: Duration = Duration::from_secs(1);
/// A rate this far from the filter's input rate (as a fraction) rebuilds the filter.
const REBUILD_TOLERANCE: f64 = 0.1;
/// A measured rate under this fraction of the configured rate is a warning.
const LOW_RATE_FRACTION: f64 = 0.9;
/// A jump of more than this many input periods between batches is a gap.
const GAP_PERIODS: f64 = 1.5;

/// Something the pipeline wants sent to the viewer.
#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    /// A text message.
    Message(ServerMessage),
    /// A binary sample frame, already encoded.
    Frame(Vec<u8>),
}

enum State {
    /// Collecting batches until enough time has passed to measure the rate.
    Measuring { held: Vec<RawSamples> },
    /// Filtering with a decimator built for `input_rate_hz`.
    Running {
        decimator: Decimator,
        input_rate_hz: f64,
    },
    /// The filter could not be built; input is ignored.
    Failed,
}

/// The filter and frame batching for one chip and one viewer.
pub struct ChipPipeline {
    name: String,
    index: u16,
    nominal_rate_hz: Option<f64>,
    max_hz: f64,
    updates_per_second: f64,
    limits: StreamLimits,
    state: State,
    last_t: Option<f64>,
    frame_t0: Option<f64>,
    frame: Vec<f32>,
    next_frame: Option<Instant>,
    next_health: Option<Instant>,
    window_times: Vec<f64>,
    dropped_batches: u64,
    overflows: Option<u64>,
    errors: Option<u64>,
    overflows_rose: bool,
    errors_rose: bool,
}

impl ChipPipeline {
    /// A pipeline for `chip` whose frames carry `index`, showing `0..=max_hz` and sending
    /// `updates_per_second` frames a second, both limited by `limits`.
    pub fn new(
        chip: &Chip,
        index: u16,
        max_hz: f64,
        updates_per_second: f64,
        limits: &StreamLimits,
    ) -> Self {
        Self {
            name: chip.section.clone(),
            index,
            nominal_rate_hz: chip.nominal_rate_hz,
            max_hz,
            updates_per_second,
            limits: limits.clone(),
            state: State::Measuring { held: Vec::new() },
            last_t: None,
            frame_t0: None,
            frame: Vec::new(),
            next_frame: None,
            next_health: None,
            window_times: Vec::new(),
            dropped_batches: 0,
            overflows: None,
            errors: None,
            overflows_rose: false,
            errors_rose: false,
        }
    }

    /// Takes a batch that arrived at `now` and returns what is ready to send.
    pub fn push(&mut self, batch: &Batch, now: Instant) -> Vec<Output> {
        let mut out = Vec::new();
        self.next_health.get_or_insert(now + HEALTH_INTERVAL);
        self.next_frame.get_or_insert(now + self.interval());
        self.note_counters(batch);
        if !is_valid(&batch.samples) || !self.is_after_last(&batch.samples) {
            self.dropped_batches += 1;
        } else {
            self.accept(batch.samples.clone(), now, &mut out);
        }
        self.housekeeping(now, &mut out);
        out
    }

    /// Flushes a frame whose update interval has passed and sends health when it is due.
    pub fn tick(&mut self, now: Instant) -> Vec<Output> {
        let mut out = Vec::new();
        self.next_health.get_or_insert(now + HEALTH_INTERVAL);
        self.housekeeping(now, &mut out);
        out
    }

    fn is_after_last(&self, samples: &RawSamples) -> bool {
        match (self.last_t, samples.t.first()) {
            (Some(last), Some(first)) => *first > last,
            _ => true,
        }
    }

    fn note_counters(&mut self, batch: &Batch) {
        if let Some(value) = batch.overflows {
            if self.overflows.is_some_and(|seen| value > seen) {
                self.overflows_rose = true;
            }
            self.overflows = Some(value);
        }
        if let Some(value) = batch.errors {
            if self.errors.is_some_and(|seen| value > seen) {
                self.errors_rose = true;
            }
            self.errors = Some(value);
        }
    }

    fn accept(&mut self, samples: RawSamples, now: Instant, out: &mut Vec<Output>) {
        let Some(last) = samples.t.last().copied() else {
            return;
        };
        match &mut self.state {
            State::Failed => {}
            State::Measuring { held } => {
                held.push(samples);
                let times: Vec<f64> = held.iter().flat_map(|s| s.t.iter().copied()).collect();
                let span = times.last().copied().unwrap_or(0.0) - times[0];
                if span >= MEASURE_SECONDS {
                    let held = std::mem::take(held);
                    self.build(rate_from_times(&times), held, now, out);
                }
            }
            State::Running { .. } => self.filter(&samples, now, out),
        }
        self.last_t = Some(last);
    }

    /// Builds the filter for `rate` and runs the batches held while measuring through it.
    fn build(
        &mut self,
        rate: Option<f64>,
        held: Vec<RawSamples>,
        now: Instant,
        out: &mut Vec<Output>,
    ) {
        let built = rate.ok_or_else(|| "the chip's timestamps are not usable".to_owned());
        let built = built.and_then(|rate| {
            let (max_hz, updates) =
                clamp_request(self.max_hz, self.updates_per_second, rate, &self.limits);
            Decimator::new(rate, max_hz)
                .map(|decimator| (rate, max_hz, updates, decimator))
                .map_err(|e| e.to_string())
        });
        match built {
            Ok((rate, max_hz, updates, decimator)) => {
                self.flush_frame(now, out);
                out.push(Output::Message(ServerMessage::Stream {
                    chip: self.name.clone(),
                    index: self.index,
                    rate_hz: decimator.output_rate_hz(),
                    input_rate_hz: rate,
                    configured_rate_hz: self.nominal_rate_hz,
                    factor: decimator.factor(),
                    max_hz,
                    updates_per_second: updates,
                }));
                self.updates_per_second = updates;
                self.state = State::Running {
                    decimator,
                    input_rate_hz: rate,
                };
                let last = self.last_t.take();
                for samples in held {
                    self.gap_check(&samples, now, out);
                    self.filter(&samples, now, out);
                    self.last_t = samples.t.last().copied();
                }
                self.last_t = self.last_t.or(last);
            }
            Err(reason) => {
                self.state = State::Failed;
                out.push(Output::Message(ServerMessage::Closed {
                    chip: self.name.clone(),
                    reason,
                }));
            }
        }
    }

    fn input_period(&self) -> Option<f64> {
        match &self.state {
            State::Running { input_rate_hz, .. } => Some(1.0 / input_rate_hz),
            _ => None,
        }
    }

    /// Ends the current frame when `samples` starts after a gap.
    fn gap_check(&mut self, samples: &RawSamples, now: Instant, out: &mut Vec<Output>) {
        if let (Some(last), Some(period), Some(first)) =
            (self.last_t, self.input_period(), samples.t.first())
            && first - last > GAP_PERIODS * period
        {
            self.flush_frame(now, out);
        }
    }

    fn filter(&mut self, samples: &RawSamples, now: Instant, out: &mut Vec<Output>) {
        self.gap_check(samples, now, out);
        let State::Running { decimator, .. } = &mut self.state else {
            return;
        };
        let kept = decimator.process(samples);
        self.window_times.extend_from_slice(&samples.t);
        for i in 0..kept.len() {
            if self.frame_t0.is_none() {
                self.frame_t0 = Some(kept.t[i]);
            }
            self.frame
                .extend_from_slice(&[kept.x[i] as f32, kept.y[i] as f32, kept.z[i] as f32]);
        }
        self.next_frame.get_or_insert(now + self.interval());
    }

    fn interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.updates_per_second)
    }

    fn flush_frame(&mut self, now: Instant, out: &mut Vec<Output>) {
        if let Some(t0) = self.frame_t0.take() {
            out.push(Output::Frame(encode_frame(self.index, t0, &self.frame)));
            self.frame.clear();
            let next = self.next_frame.unwrap_or(now) + self.interval();
            self.next_frame = Some(if next > now {
                next
            } else {
                now + self.interval()
            });
        }
    }

    fn housekeeping(&mut self, now: Instant, out: &mut Vec<Output>) {
        if self.frame_t0.is_some() && self.next_frame.is_some_and(|due| now >= due) {
            self.flush_frame(now, out);
        }
        if self.next_health.is_some_and(|due| now >= due) {
            self.next_health = Some(now + HEALTH_INTERVAL);
            self.health(now, out);
        }
    }

    fn health(&mut self, now: Instant, out: &mut Vec<Output>) {
        let measured = match &self.state {
            State::Measuring { held } => {
                let times: Vec<f64> = held.iter().flat_map(|s| s.t.iter().copied()).collect();
                rate_from_times(&times)
            }
            _ => rate_from_times(&self.window_times),
        };
        self.window_times.clear();

        if let (Some(rate), State::Running { input_rate_hz, .. }) = (measured, &self.state)
            && (rate - input_rate_hz).abs() > REBUILD_TOLERANCE * input_rate_hz
        {
            self.build(Some(rate), Vec::new(), now, out);
        }

        let mut warnings = Vec::new();
        if let (Some(rate), Some(nominal)) = (measured, self.nominal_rate_hz)
            && rate < LOW_RATE_FRACTION * nominal
        {
            warnings.push(format!(
                "measured rate {rate:.0} Hz is below the configured {nominal:.0} Hz"
            ));
        }
        if std::mem::take(&mut self.overflows_rose) {
            warnings.push("batches were dropped (overflows)".to_owned());
        }
        if std::mem::take(&mut self.errors_rose) {
            warnings.push("read errors".to_owned());
        }
        let dropped = std::mem::take(&mut self.dropped_batches);
        if dropped > 0 {
            warnings.push(format!(
                "{dropped} batches were ignored (bad or out-of-order samples)"
            ));
        }
        out.push(Output::Message(ServerMessage::Health {
            chip: self.name.clone(),
            measured_rate_hz: measured.unwrap_or(0.0),
            configured_rate_hz: self.nominal_rate_hz,
            overflows: self.overflows,
            errors: self.errors,
            dropped_frames: 0,
            warnings,
        }));
    }
}

/// Whether `s` has equal column lengths, finite values and strictly increasing timestamps.
fn is_valid(s: &RawSamples) -> bool {
    let n = s.t.len();
    n > 0
        && s.x.len() == n
        && s.y.len() == n
        && s.z.len() == n
        && [&s.t, &s.x, &s.y, &s.z]
            .iter()
            .all(|c| c.iter().all(|v| v.is_finite()))
        && s.t.windows(2).all(|w| w[1] > w[0])
}
