//! One recorder per chip: it reads the chip's bridge, keeps the current segment's samples and
//! judges them.
//!
//! The runner calls [`Recorder::begin_segment`], waits with [`Recorder::wait_past`] for data past
//! the window end and takes the segment with [`Recorder::take`]. Between segments nothing is
//! kept. Dropping a recorder closes its connection.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use wobblescope_core::RawSamples;
use wobblescope_core::eligibility::{Coverage, Health};
use wobblescope_core::numeric::median_rate;
use wobblescope_session::codec::MAX_SAMPLES;

use super::availability::OpenChip;
use crate::moonraker::bridge::{Batch, Bridge, Event};
use crate::moonraker::chips::ChipType;

/// A gap longer than this many median sample intervals inside a window degrades a chip.
const GAP_FACTOR: f64 = 3.0;

/// The most often a chip's signal level is sent, as sample time between events, in seconds.
const LEVEL_PERIOD: f64 = 0.25;

/// The signal level of one chip: RMS per axis after removing the batch mean, in the chip's
/// units (mm/s²).
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    /// The chip's config section.
    pub chip: String,
    /// RMS of x, y and z.
    pub rms: [f64; 3],
}

/// Why a chip did not deliver data past a window end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// Nothing newer than the window end arrived in time.
    NoData,
    /// The connection closed, with the reason.
    Closed(String),
}

/// What one chip's segment looked like.
#[derive(Debug, Clone, PartialEq)]
pub struct ChipSegmentReport {
    /// Whether the samples cover the window.
    pub coverage: Coverage,
    /// Whether this segment shows a problem: counters rose, or a gap sits inside the window.
    pub degraded: bool,
    /// The segment's health on its own: `Degraded` as above, `Ok` for a chip that reports
    /// counters, `Unknown` for one that does not.
    pub health: Health,
    /// What was seen, one line each.
    pub notes: Vec<String>,
    /// The sample rate seen in this segment, in Hz.
    pub observed_rate_hz: Option<f64>,
    /// Whether the buffer hit its cap and stopped accepting samples.
    pub capped: bool,
}

/// A segment's samples and its report.
pub struct Taken {
    /// The samples, in arrival order.
    pub samples: RawSamples,
    /// The judgement.
    pub report: ChipSegmentReport,
}

/// A chip's health over the whole test.
#[derive(Debug, Clone, PartialEq)]
pub struct ChipSummary {
    /// `Ok` and `Degraded` as the counters and gaps show; `Unknown` for a chip type without
    /// counters whose timestamps were clean.
    pub health: Health,
    /// The notes of every segment that showed a problem.
    pub notes: Vec<String>,
    /// The median of the segments' sample rates, in Hz.
    pub observed_rate_hz: Option<f64>,
}

#[derive(Default)]
struct Inner {
    active: bool,
    samples: RawSamples,
    capped: bool,
    errors: Option<u64>,
    overflows: Option<u64>,
    base_errors: Option<u64>,
    base_overflows: Option<u64>,
    counters_seen: bool,
    // Aggregates across taken segments.
    degraded: bool,
    notes: Vec<String>,
    rates: Vec<f64>,
    taken: usize,
    last_level_t: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct Progress {
    last_t: Option<f64>,
    closed: Option<String>,
}

struct Shared {
    name: String,
    cap: usize,
    inner: Mutex<Inner>,
    progress: watch::Sender<Progress>,
    levels: Option<mpsc::UnboundedSender<Level>>,
}

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A running recorder for one chip.
pub struct Recorder {
    shared: Arc<Shared>,
    watch: watch::Receiver<Progress>,
    chip_type: ChipType,
    task: JoinHandle<()>,
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Recorder {
    /// Starts recording `open`. Level events go to `levels` when given.
    #[must_use]
    pub fn spawn(open: OpenChip, levels: Option<mpsc::UnboundedSender<Level>>) -> Self {
        Self::spawn_capped(open, levels, MAX_SAMPLES)
    }

    /// As [`Recorder::spawn`], with a smaller sample cap per segment.
    #[must_use]
    pub fn spawn_capped(
        open: OpenChip,
        levels: Option<mpsc::UnboundedSender<Level>>,
        cap: usize,
    ) -> Self {
        let (progress, watch) = watch::channel(Progress::default());
        let shared = Arc::new(Shared {
            name: open.chip.section.clone(),
            cap,
            inner: Mutex::new(Inner::default()),
            progress,
            levels,
        });
        let chip_type = open.chip.chip_type;
        // The batches that proved the chip set the counter baseline and the last timestamp, but
        // no segment is active yet, so their samples are not kept.
        for batch in &open.first_batches {
            shared.apply(batch);
        }
        let task = tokio::spawn(read(shared.clone(), open.bridge));
        Self {
            shared,
            watch,
            chip_type,
            task,
        }
    }

    /// The chip's config section.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.shared.name
    }

    /// The chip's type.
    #[must_use]
    pub fn chip_type(&self) -> ChipType {
        self.chip_type
    }

    /// The newest timestamp received.
    #[must_use]
    pub fn last_timestamp(&self) -> Option<f64> {
        self.watch.borrow().last_t
    }

    /// Why the connection closed, once it has.
    #[must_use]
    pub fn closed(&self) -> Option<String> {
        self.watch.borrow().closed.clone()
    }

    /// Resolves with the reason once the connection closes.
    pub async fn lost(&self) -> String {
        let mut rx = self.watch.clone();
        loop {
            if let Some(reason) = rx.borrow_and_update().closed.clone() {
                return reason;
            }
            if rx.changed().await.is_err() {
                return "the recorder stopped".to_owned();
            }
        }
    }

    /// Starts a segment: clears the buffer and remembers the counters as its baseline.
    pub fn begin_segment(&self) {
        let mut i = lock(&self.shared.inner);
        i.active = true;
        i.samples = RawSamples::default();
        i.capped = false;
        i.base_errors = i.errors;
        i.base_overflows = i.overflows;
    }

    /// Waits up to `timeout` for a sample newer than `t`.
    ///
    /// # Errors
    /// [`Missing::NoData`] when the time runs out, [`Missing::Closed`] when the connection
    /// closed first.
    pub async fn wait_past(&self, t: f64, timeout: Duration) -> Result<(), Missing> {
        let mut rx = self.watch.clone();
        let wait = async {
            loop {
                {
                    let p = rx.borrow_and_update();
                    if p.last_t.is_some_and(|last| last > t) {
                        return Ok(());
                    }
                    if let Some(reason) = &p.closed {
                        return Err(Missing::Closed(reason.clone()));
                    }
                }
                if rx.changed().await.is_err() {
                    return Err(Missing::Closed("the recorder stopped".to_owned()));
                }
            }
        };
        tokio::time::timeout(timeout, wait)
            .await
            .unwrap_or(Err(Missing::NoData))
    }

    /// Ends the segment and returns its samples and report for the window `start..end`.
    #[must_use]
    pub fn take(&self, start: f64, end: f64) -> Taken {
        let mut i = lock(&self.shared.inner);
        i.active = false;
        let samples = std::mem::take(&mut i.samples);
        let report = judge(&i, &samples, start, end);
        i.degraded |= report.degraded;
        i.taken += 1;
        let prefix = format!("segment {}: ", i.taken);
        i.notes
            .extend(report.notes.iter().map(|n| format!("{prefix}{n}")));
        if let Some(rate) = report.observed_rate_hz {
            i.rates.push(rate);
        }
        Taken { samples, report }
    }

    /// The chip's health over every segment taken so far.
    #[must_use]
    pub fn summary(&self) -> ChipSummary {
        let i = lock(&self.shared.inner);
        let health = if i.degraded {
            Health::Degraded
        } else if i.counters_seen {
            Health::Ok
        } else {
            Health::Unknown
        };
        let mut rates = i.rates.clone();
        rates.sort_by(f64::total_cmp);
        ChipSummary {
            health,
            notes: i.notes.clone(),
            observed_rate_hz: rates.get(rates.len() / 2).copied(),
        }
    }
}

impl Shared {
    /// Applies one batch: counters, the buffer when a segment is active, the level.
    fn apply(&self, batch: &Batch) {
        let mut i = lock(&self.inner);
        if batch.errors.is_some() || batch.overflows.is_some() {
            i.counters_seen = true;
            i.errors = batch.errors.or(i.errors);
            i.overflows = batch.overflows.or(i.overflows);
            // The first batch of the connection sets the baseline for the first segment.
            i.base_errors = i.base_errors.or(batch.errors);
            i.base_overflows = i.base_overflows.or(batch.overflows);
        }
        if i.active && !i.capped {
            let room = self.cap.saturating_sub(i.samples.len());
            let n = batch.samples.len().min(room);
            i.samples.t.extend_from_slice(&batch.samples.t[..n]);
            i.samples.x.extend_from_slice(&batch.samples.x[..n]);
            i.samples.y.extend_from_slice(&batch.samples.y[..n]);
            i.samples.z.extend_from_slice(&batch.samples.z[..n]);
            if n < batch.samples.len() {
                i.capped = true;
            }
        }
        let last = batch.samples.t.last().copied();
        if let (Some(last), Some(levels)) = (last, &self.levels)
            && i.last_level_t.is_none_or(|t| last >= t + LEVEL_PERIOD)
        {
            i.last_level_t = Some(last);
            let _ = levels.send(Level {
                chip: self.name.clone(),
                rms: rms_without_mean(&batch.samples),
            });
        }
        drop(i);
        if let Some(last) = last {
            self.progress.send_modify(|p| p.last_t = Some(last));
        }
    }
}

async fn read(shared: Arc<Shared>, mut bridge: Bridge) {
    loop {
        match bridge.recv().await {
            Event::Batch(batch) => shared.apply(&batch),
            Event::Closed(reason) => {
                shared.progress.send_modify(|p| p.closed = Some(reason));
                return;
            }
        }
    }
}

fn rms_without_mean(samples: &RawSamples) -> [f64; 3] {
    let rms = |column: &[f64]| {
        if column.is_empty() {
            return 0.0;
        }
        let n = column.len() as f64;
        let mean = column.iter().sum::<f64>() / n;
        (column.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n).sqrt()
    };
    [rms(&samples.x), rms(&samples.y), rms(&samples.z)]
}

fn judge(i: &Inner, samples: &RawSamples, start: f64, end: f64) -> ChipSegmentReport {
    let mut notes = Vec::new();
    let mut degraded = false;

    let rise = |now: Option<u64>, base: Option<u64>| match (now, base) {
        (Some(now), Some(base)) => now.saturating_sub(base),
        _ => 0,
    };
    for (name, now, base) in [
        ("errors", i.errors, i.base_errors),
        ("overflows", i.overflows, i.base_overflows),
    ] {
        let by = rise(now, base);
        if by > 0 {
            degraded = true;
            notes.push(format!("the chip's {name} counter rose by {by}"));
        }
    }

    let rate = median_rate(&samples.t);
    if let Some(interval) = rate.map(|r| 1.0 / r) {
        let limit = interval * GAP_FACTOR;
        for pair in samples.t.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if b > start && a < end && b - a > limit {
                degraded = true;
                notes.push(format!(
                    "a gap of {:.1} ms at t={a:.3} s, {:.1} times the sample interval",
                    (b - a) * 1000.0,
                    (b - a) / interval
                ));
            }
        }
    }

    let coverage = match (samples.t.first(), samples.t.last()) {
        (Some(&first), Some(&last)) => {
            let mut missing = Vec::new();
            if first > start {
                missing.push(format!(
                    "the first sample is {:.3} s after the window start",
                    first - start
                ));
            }
            if last < end {
                missing.push(format!(
                    "the last sample is {:.3} s before the window end; no data after the window \
                     end",
                    end - last
                ));
            }
            if missing.is_empty() {
                Coverage::Complete
            } else {
                Coverage::Incomplete {
                    missing: missing.join("; "),
                }
            }
        }
        _ => Coverage::Incomplete {
            missing: "no samples in the segment; no data after the window end".to_owned(),
        },
    };
    if i.capped {
        notes.push("the segment hit the sample limit and the rest was dropped".to_owned());
    }
    let health = if degraded {
        Health::Degraded
    } else if i.counters_seen {
        Health::Ok
    } else {
        Health::Unknown
    };
    ChipSegmentReport {
        coverage,
        degraded,
        health,
        notes,
        observed_rate_hz: rate,
        capped: i.capped,
    }
}
