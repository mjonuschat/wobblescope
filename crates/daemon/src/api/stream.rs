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
use tokio::sync::{Notify, mpsc};
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
}

impl Outbox {
    fn new() -> Self {
        Self {
            queue: Mutex::new(OutQueue::new(QUEUE_CAPACITY)),
            wake: Notify::new(),
            overflowed: AtomicBool::new(false),
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
    settings: mpsc::UnboundedSender<(f64, f64)>,
    requested: (f64, f64),
}

/// The viewer's subscriptions. Dropping it aborts every chip task, and with them every bridge.
#[derive(Default)]
struct Subscriptions(HashMap<String, Subscription>);

impl Subscriptions {
    /// Forgets chips whose task has ended, for example because its bridge closed.
    fn prune(&mut self) {
        self.0.retain(|_, s| !s.task.is_finished());
    }

    fn free_index(&self) -> u16 {
        (0..)
            .find(|i| self.0.values().all(|s| s.index != *i))
            .unwrap_or(0)
    }
}

impl Drop for Subscriptions {
    fn drop(&mut self) {
        for subscription in self.0.values() {
            subscription.task.abort();
        }
    }
}

fn is_printing(state: Option<&str>) -> bool {
    matches!(state, Some("printing" | "paused"))
}

async fn serve(mut socket: WebSocket, state: ApiState) {
    let settings = state.settings.clone();
    let moonraker = state.moonraker.clone();
    let limits = StreamLimits::from_settings(&settings);
    let outbox = Arc::new(Outbox::new());
    let mut subscriptions = Subscriptions::default();

    let printing = match moonraker.control() {
        Some(control) => read_printer_state(&control)
            .await
            .is_ok_and(|state| is_printing(state.print_state.as_deref())),
        None => false,
    };
    outbox.push(Entry::Reliable(ServerMessage::Hello {
        printing,
        defaults: StreamDefaults::for_state(&settings, printing),
        limits: limits.clone(),
    }));

    loop {
        while let Some(entry) = outbox.pop() {
            if socket.send(wire(entry)).await.is_err() {
                return;
            }
        }
        if outbox.overflowed.load(Ordering::Acquire) {
            let close = CloseFrame {
                code: close_code::POLICY,
                reason: TOO_SLOW.into(),
            };
            let _ = socket.send(Message::Close(Some(close))).await;
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
            () = outbox.wake.notified() => {}
        }
    }
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
            if let Some(subscription) = subscriptions.0.remove(&chip) {
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
            if let Some(subscription) = subscriptions.0.get_mut(&chip) {
                if subscription.requested == requested {
                    return;
                }
                if subscription.settings.send(requested).is_ok() {
                    subscription.requested = requested;
                    return;
                }
                subscriptions.0.remove(&chip);
            }
            if subscriptions.0.len() >= limits.max_chips {
                return refuse(&format!(
                    "at most {} accelerometers can be streamed at once",
                    limits.max_chips
                ));
            }
            let index = subscriptions.free_index();
            let (settings, changes) = mpsc::unbounded_channel();
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
            subscriptions.0.insert(
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
async fn run_chip(
    run: ChipRun,
    requested: (f64, f64),
    changes: mpsc::UnboundedReceiver<(f64, f64)>,
) {
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
    mut changes: mpsc::UnboundedReceiver<(f64, f64)>,
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
            change = changes.recv() => match change {
                Some(values) => {
                    pipe = pipeline(values);
                    continue;
                }
                None => return None,
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
