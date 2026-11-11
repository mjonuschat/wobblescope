//! `GET /api/stream`: a websocket that streams accelerometer data to a viewer.
//!
//! The viewer subscribes to chips. Each subscribed chip has one task that owns its bridge and
//! its [`ChipPipeline`] and puts what the pipeline produces on the connection's [`OutQueue`].
//! The queue is bounded and producers never wait on it, so a slow viewer cannot hold up the
//! bridges that drain Klipper.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::response::Response;
use tokio::sync::{Notify, broadcast, watch};
use tokio::task::JoinHandle;

use super::ApiState;
use crate::capture::availability::AVAILABILITY_TIMEOUT;
use crate::capture::preflight::read_printer_state;
use crate::moonraker::Moonraker;
use crate::moonraker::bridge::{Bridge, BridgeError, Event};
use crate::moonraker::chips::Chip;
use crate::stream::pipeline::{ChipPipeline, Output};
use crate::stream::protocol::{
    ClientMessage, ServerMessage, StreamDefaults, StreamLimits, clamp_request,
};
use crate::stream::queue::{Entry, OutQueue, Push};

/// The most entries waiting for one viewer.
const QUEUE_CAPACITY: usize = 32;
/// How long the call that fetches a one-shot token may take.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a chip task lets its pipeline flush a due frame and report health.
const TICK: Duration = Duration::from_millis(25);
/// How long `Hello` waits for the printer state before assuming no print is running.
const PRINT_STATE_TIMEOUT: Duration = Duration::from_secs(2);
/// How long one websocket write to a viewer may take before the viewer is dropped.
pub(super) const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Why a viewer that cannot keep up is disconnected.
const TOO_SLOW: &str = "viewer too slow";

pub(super) async fn stream(State(state): State<ApiState>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| serve(socket, state))
}

/// The connection's outgoing queue, shared by the handler and the chip tasks.
struct Outbox {
    queue: Mutex<OutQueue>,
    wake: Notify,
    overflowed: AtomicBool,
    overflow: Notify,
}

impl Outbox {
    fn new() -> Self {
        Self {
            queue: Mutex::new(OutQueue::new(QUEUE_CAPACITY)),
            wake: Notify::new(),
            overflowed: AtomicBool::new(false),
            overflow: Notify::new(),
        }
    }

    fn push(&self, entry: Entry) {
        let pushed = self
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(entry);
        self.after_push(pushed);
    }

    fn after_push(&self, pushed: Push) {
        if pushed == Push::Overflow {
            self.overflowed.store(true, Ordering::Release);
            self.overflow.notify_one();
        }
        self.wake.notify_one();
    }

    /// Queues what a pipeline produced. Health carries the queue's dropped frame count.
    fn emit(&self, outputs: Vec<Output>) {
        for output in outputs {
            let mut queue = self
                .queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let pushed = match output {
                Output::Frame(bytes) => queue.push(Entry::Frame(bytes)),
                Output::Message(ServerMessage::Health {
                    chip,
                    measured_rate_hz,
                    configured_rate_hz,
                    overflows,
                    errors,
                    warnings,
                    ..
                }) => {
                    let message = ServerMessage::Health {
                        chip: chip.clone(),
                        measured_rate_hz,
                        configured_rate_hz,
                        overflows,
                        errors,
                        dropped_frames: queue.dropped_frames(),
                        warnings,
                    };
                    queue.push(Entry::Health(chip, message))
                }
                Output::Message(message) => queue.push(Entry::Reliable(message)),
            };
            drop(queue);
            self.after_push(pushed);
        }
    }

    /// Resolves once the queue has overflowed.
    async fn overflowed(&self) {
        let notified = self.overflow.notified();
        if !self.overflowed.load(Ordering::Acquire) {
            notified.await;
        }
    }

    fn pop(&self) -> Option<Entry> {
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop()
    }
}

/// A subscribed chip: its task, the way to change its settings, and the values it runs with.
struct Subscription {
    task: JoinHandle<()>,
    index: u16,
    settings: watch::Sender<(f64, f64)>,
    requested: (f64, f64),
}

/// The viewer's subscriptions. Dropping it aborts every chip task, and with them every bridge.
#[derive(Default)]
struct Subscriptions {
    chips: HashMap<String, Subscription>,
    next_index: u16,
}

impl Subscriptions {
    /// Forgets chips whose task has ended, for example because its bridge closed.
    fn prune(&mut self) {
        self.chips.retain(|_, s| !s.task.is_finished());
    }

    /// The next frame index. Indices only count up, so a frame still queued for a chip that
    /// was unsubscribed is never read as the chip that took its place. Wraps after 65535 and
    /// skips indices still in use.
    fn take_index(&mut self) -> u16 {
        loop {
            let index = self.next_index;
            self.next_index = self.next_index.wrapping_add(1);
            if self.chips.values().all(|s| s.index != index) {
                return index;
            }
        }
    }
}

impl Drop for Subscriptions {
    fn drop(&mut self) {
        for subscription in self.chips.values() {
            subscription.task.abort();
        }
    }
}

fn is_printing(state: Option<&str>) -> bool {
    matches!(state, Some("printing" | "paused"))
}

async fn serve(mut socket: WebSocket, state: ApiState) {
    let settings = state.settings.clone();
    let write_timeout = state.capture.stream_write_timeout;
    let moonraker = state.moonraker.clone();
    let limits = StreamLimits::from_settings(&settings);
    let outbox = Arc::new(Outbox::new());
    let mut subscriptions = Subscriptions::default();
    let mut console = moonraker.gcode_lines();

    let printing = match moonraker.control() {
        Some(control) => tokio::time::timeout(PRINT_STATE_TIMEOUT, read_printer_state(&control))
            .await
            .is_ok_and(|read| read.is_ok_and(|state| is_printing(state.print_state.as_deref()))),
        None => false,
    };
    outbox.push(Entry::Reliable(ServerMessage::Hello {
        printing,
        defaults: StreamDefaults::for_state(&settings, printing),
        limits: limits.clone(),
    }));

    loop {
        while let Some(entry) = outbox.pop() {
            if !send_bounded(&mut socket, wire(entry), write_timeout, &outbox).await {
                return;
            }
        }
        if let Some(close) = overflow_close(&outbox) {
            let _ = tokio::time::timeout(write_timeout, socket.send(close)).await;
            return;
        }
        tokio::select! {
            incoming = socket.recv() => match incoming {
                None | Some(Err(_) | Ok(Message::Close(_))) => return,
                Some(Ok(Message::Text(text))) => {
                    if let Ok(message) = serde_json::from_str::<ClientMessage>(&text) {
                        handle(message, &mut subscriptions, &moonraker, &limits, &outbox);
                    }
                }
                Some(Ok(_)) => {}
            },
            line = console.recv() => match line {
                Ok(line) => outbox.push(Entry::Console(vec![line])),
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => console = moonraker.gcode_lines(),
            },
            () = outbox.wake.notified() => {}
        }
    }
}

/// Writes one message. A write that fails, outlasts `timeout` or is still blocked when the
/// queue overflows ends the connection, so `false` means the caller must return.
async fn send_bounded(
    socket: &mut WebSocket,
    message: Message,
    timeout: Duration,
    outbox: &Outbox,
) -> bool {
    tokio::select! {
        sent = tokio::time::timeout(timeout, socket.send(message)) => sent.is_ok_and(|sent| sent.is_ok()),
        () = outbox.overflowed() => false,
    }
}

/// The close message to send when the viewer's queue has overflowed, `None` while it has not.
fn overflow_close(outbox: &Outbox) -> Option<Message> {
    outbox
        .overflowed
        .load(Ordering::Acquire)
        .then(too_slow_close)
}

/// The close message for a viewer whose queue overflowed.
fn too_slow_close() -> Message {
    Message::Close(Some(CloseFrame {
        code: close_code::POLICY,
        reason: TOO_SLOW.into(),
    }))
}

fn wire(entry: Entry) -> Message {
    let text = |message: &ServerMessage| {
        Message::text(serde_json::to_string(message).unwrap_or_else(|_| "{}".to_owned()))
    };
    match entry {
        Entry::Frame(bytes) => Message::Binary(bytes.into()),
        Entry::Health(_, message) | Entry::Reliable(message) => text(&message),
        Entry::Console(lines) => text(&ServerMessage::Console { lines }),
    }
}

fn handle(
    message: ClientMessage,
    subscriptions: &mut Subscriptions,
    moonraker: &Moonraker,
    limits: &StreamLimits,
    outbox: &Arc<Outbox>,
) {
    match message {
        ClientMessage::Unsubscribe { chip } => {
            if let Some(subscription) = subscriptions.chips.remove(&chip) {
                subscription.task.abort();
            }
        }
        ClientMessage::Subscribe {
            chip,
            max_hz,
            updates_per_second,
        } => {
            let refuse = |reason: &str| {
                outbox.push(Entry::Reliable(ServerMessage::Refused {
                    chip: chip.clone(),
                    reason: reason.to_owned(),
                }));
            };
            subscriptions.prune();
            if moonraker.control().is_none() {
                return refuse("Moonraker is not connected");
            }
            let Some(known) = moonraker
                .chips()
                .list
                .chips
                .into_iter()
                .find(|c| c.section == chip)
            else {
                return refuse("no such accelerometer");
            };
            let requested = clamp_request(max_hz, updates_per_second, f64::INFINITY, limits);
            if let Some(subscription) = subscriptions.chips.get_mut(&chip) {
                if subscription.requested == requested {
                    return;
                }
                if subscription.settings.send(requested).is_ok() {
                    subscription.requested = requested;
                    return;
                }
                subscriptions.chips.remove(&chip);
            }
            if subscriptions.chips.len() >= limits.max_chips {
                return refuse(&format!(
                    "at most {} accelerometers can be streamed at once",
                    limits.max_chips
                ));
            }
            let index = subscriptions.take_index();
            let (settings, changes) = watch::channel(requested);
            let task = tokio::spawn(run_chip(
                ChipRun {
                    chip: known,
                    index,
                    limits: limits.clone(),
                    moonraker: moonraker.clone(),
                    outbox: outbox.clone(),
                },
                requested,
                changes,
            ));
            subscriptions.chips.insert(
                chip,
                Subscription {
                    task,
                    index,
                    settings,
                    requested,
                },
            );
        }
    }
}

struct ChipRun {
    chip: Chip,
    index: u16,
    limits: StreamLimits,
    moonraker: Moonraker,
    outbox: Arc<Outbox>,
}

/// Runs one chip's stream and, when it ends for a reason the viewer should see, reports it.
async fn run_chip(run: ChipRun, requested: (f64, f64), changes: watch::Receiver<(f64, f64)>) {
    if let Some(reason) = drive(&run, requested, changes).await {
        run.outbox.push(Entry::Reliable(ServerMessage::Closed {
            chip: run.chip.section.clone(),
            reason,
        }));
    }
}

/// Opens the chip's bridge and feeds its pipeline until the bridge ends. The reason it ended,
/// or `None` when the pipeline already reported the end itself.
async fn drive(
    run: &ChipRun,
    requested: (f64, f64),
    mut changes: watch::Receiver<(f64, f64)>,
) -> Option<String> {
    let mut bridge = match open_bridge(run).await {
        Ok(bridge) => bridge,
        Err(reason) => return Some(reason),
    };
    let pipeline =
        |(max_hz, updates)| ChipPipeline::new(&run.chip, run.index, max_hz, updates, &run.limits);
    let mut pipe = pipeline(requested);
    let mut ticker = tokio::time::interval(TICK);
    loop {
        let outputs = tokio::select! {
            event = bridge.recv() => match event {
                Event::Batch(batch) => pipe.push(&batch, Instant::now()),
                Event::Closed(reason) => return Some(reason),
            },
            changed = changes.changed() => match changed {
                Ok(()) => {
                    pipe = pipeline(*changes.borrow_and_update());
                    continue;
                }
                Err(_) => return None,
            },
            _ = ticker.tick() => pipe.tick(Instant::now()),
        };
        let ended = outputs
            .iter()
            .any(|o| matches!(o, Output::Message(ServerMessage::Closed { .. })));
        run.outbox.emit(outputs);
        if ended {
            return None;
        }
    }
}

async fn open_bridge(run: &ChipRun) -> Result<Bridge, String> {
    let control = run
        .moonraker
        .control()
        .ok_or_else(|| "Moonraker is not connected".to_owned())?;
    let token = control
        .call("access.oneshot_token", None, TOKEN_TIMEOUT)
        .await
        .map_err(|e| format!("could not get a bridge token from Moonraker: {e}"))?;
    let token = token
        .as_str()
        .ok_or_else(|| "Moonraker's bridge token is not a string".to_owned())?;
    let url = run.moonraker.bridge_url(token);
    Bridge::open(
        &url,
        &run.chip.dump_endpoint,
        run.chip.sensor.as_deref(),
        AVAILABILITY_TIMEOUT,
    )
    .await
    .map_err(|e| match e {
        BridgeError::Subscribe(message) => message,
        other => other.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscription(index: u16) -> Subscription {
        Subscription {
            task: tokio::spawn(std::future::pending()),
            index,
            settings: watch::channel((1.0, 1.0)).0,
            requested: (1.0, 1.0),
        }
    }

    #[tokio::test]
    async fn a_new_chip_does_not_take_the_index_of_an_unsubscribed_one() {
        let mut subscriptions = Subscriptions::default();
        let first = subscriptions.take_index();
        subscriptions
            .chips
            .insert("adxl345".to_owned(), subscription(first));
        subscriptions.chips.remove("adxl345");
        let second = subscriptions.take_index();
        assert_ne!(first, second);
    }

    #[tokio::test]
    async fn indices_skip_those_still_in_use_after_wrapping() {
        let mut subscriptions = Subscriptions::default();
        subscriptions.next_index = u16::MAX;
        subscriptions.chips.insert("a".to_owned(), subscription(0));
        assert_eq!(subscriptions.take_index(), u16::MAX);
        assert_eq!(subscriptions.take_index(), 1);
    }

    #[test]
    fn health_carries_the_frames_the_queue_dropped() {
        let outbox = Outbox::new();
        let frames = (0..QUEUE_CAPACITY + 3)
            .map(|_| Output::Frame(vec![0]))
            .collect();
        outbox.emit(frames);
        outbox.emit(vec![Output::Message(ServerMessage::Health {
            chip: "adxl345".to_owned(),
            measured_rate_hz: 0.0,
            configured_rate_hz: None,
            overflows: None,
            errors: None,
            dropped_frames: 0,
            warnings: vec![],
        })]);
        let mut dropped = None;
        while let Some(entry) = outbox.pop() {
            if let Entry::Health(_, ServerMessage::Health { dropped_frames, .. }) = entry {
                dropped = Some(dropped_frames);
            }
        }
        assert!(dropped.is_some_and(|n| n >= 3), "{dropped:?}");
        assert!(!outbox.overflowed.load(Ordering::Acquire));
    }

    /// A socket-level test cannot reach this: the handler pops and sends every entry it queues
    /// before it reads the next client message, so only a blocked socket lets reliable entries
    /// pile up, and a blocked handler accepts no new subscriptions to produce them.
    #[test]
    fn a_queue_full_of_reliable_entries_overflows_into_a_too_slow_close() {
        let outbox = Outbox::new();
        let refusal = || {
            Entry::Reliable(ServerMessage::Refused {
                chip: "adxl345".to_owned(),
                reason: "no such accelerometer".to_owned(),
            })
        };
        for _ in 0..QUEUE_CAPACITY {
            outbox.push(refusal());
        }
        assert!(overflow_close(&outbox).is_none());
        outbox.push(refusal());

        let Some(Message::Close(Some(close))) = overflow_close(&outbox) else {
            panic!("a close message with a frame");
        };
        assert_eq!(close.code, close_code::POLICY);
        assert_eq!(close.reason.as_str(), "viewer too slow");
    }
}
