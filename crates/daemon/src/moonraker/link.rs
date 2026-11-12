//! The connection supervisor: one task that keeps the websocket to Moonraker, tracks Klipper's
//! state and the printer's accelerometers, and publishes what it learns.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use super::discovery::{RootsError, parse_config_root, parse_configfile};
use super::rpc::{Notification, Rpc, RpcError};
use super::status::{KlippyState, MoonrakerState, state_line};
use super::{Publisher, Timings};

/// The project URL Moonraker requires in `server.connection.identify`.
const PROJECT_URL: &str = "https://github.com/mjonuschat/wobblescope";

const UNAUTHORIZED_MESSAGE: &str = "Moonraker refused the daemon. Add the daemon's address to \
    trusted_clients in moonraker.conf, or set moonraker_api_key in the daemon's config.";

/// How a connection ended.
enum Failure {
    /// The connection failed or timed out.
    Unreachable(String),
    /// Moonraker wants authentication.
    Unauthorized,
    /// Nobody is listening for the status any more.
    Stop,
}

/// Where the config root goes once Moonraker names it.
pub(super) type RootReport = oneshot::Sender<Result<PathBuf, RootsError>>;

pub(super) struct Link {
    pub(super) ws_url: String,
    pub(super) base_url: String,
    pub(super) api_key: Option<String>,
    pub(super) timings: Timings,
    pub(super) publisher: Publisher,
    pub(super) root_report: Option<RootReport>,
    /// Sent by the access task when Moonraker refused one of its calls.
    pub(super) refused: mpsc::UnboundedReceiver<()>,
}

/// What the supervisor tracks about the current connection.
struct Tracker {
    state: KlippyState,
    message: Option<String>,
    chips_current: bool,
    /// Counts changes of Klipper's state, so a chip read can tell the state moved under it.
    changes: u64,
}

impl Link {
    /// Runs until nobody listens for the status.
    pub(super) async fn run(mut self) {
        let mut backoff = self.timings.backoff_start;
        loop {
            if self.publisher.status.is_closed() {
                return;
            }
            let failure = self.session(&mut backoff).await;
            self.publisher.set_control(None);
            match failure {
                Failure::Stop => return,
                failure => self.publish_failure(&failure),
            }
            tokio::select! {
                () = tokio::time::sleep(backoff) => {}
                () = self.publisher.status.closed() => return,
            }
            backoff = (backoff * 2).min(self.timings.backoff_max);
        }
    }

    /// One connection, from connecting to its end.
    async fn session(&mut self, backoff: &mut Duration) -> Failure {
        match self.serve(backoff).await {
            Ok(never) => match never {},
            Err(failure) => failure,
        }
    }

    async fn serve(&mut self, backoff: &mut Duration) -> Result<std::convert::Infallible, Failure> {
        let t = self.timings;
        let (rpc, mut notes) = Rpc::connect(&self.ws_url, t.connect).await.map_err(|e| {
            Failure::Unreachable(format!(
                "cannot connect to Moonraker at {}: {e}",
                self.base_url
            ))
        })?;

        let mut identify = json!({
            "client_name": "wobblescope",
            "version": env!("CARGO_PKG_VERSION"),
            "type": "other",
            "url": PROJECT_URL,
        });
        if let Some(key) = &self.api_key {
            identify["api_key"] = json!(key);
        }
        self.call(
            &rpc,
            "server.connection.identify",
            Some(identify),
            t.request,
        )
        .await?;
        let info = self.call(&rpc, "server.info", None, t.request).await?;

        *backoff = t.backoff_start;
        let version = info
            .get("moonraker_version")
            .and_then(Value::as_str)
            .map(str::to_owned);
        self.publish_connected(version);
        // A refusal seen on an earlier connection says nothing about this one.
        while self.refused.try_recv().is_ok() {}
        self.publisher.set_control(Some(rpc.clone()));

        if self.root_report.is_some() {
            let roots = self
                .call(&rpc, "server.files.roots", None, t.request)
                .await?;
            if let Some(report) = self.root_report.take() {
                let _ = report.send(parse_config_root(&roots));
            }
        }

        let mut tracker = Tracker {
            state: KlippyState::Unknown,
            message: None,
            chips_current: false,
            changes: 0,
        };
        let state = klippy_state(&info);
        let message = self.state_message(&rpc, state).await?;
        let mut reread = self.apply(&mut tracker, state, Some(message));
        let mut next_poll = Instant::now() + poll_interval(&t, tracker.state);
        loop {
            while reread {
                reread = self
                    .read_chips(&rpc, &mut notes, &mut tracker, &mut next_poll)
                    .await?;
            }
            tokio::select! {
                note = notes.recv() => {
                    let Some(note) = note else {
                        return Err(self.lost(&RpcError::Closed));
                    };
                    reread = self.on_notification(&mut tracker, &note, &mut next_poll);
                }
                () = tokio::time::sleep_until(next_poll) => {
                    let info = self.call(&rpc, "server.info", None, t.request).await?;
                    let state = klippy_state(&info);
                    let message = self.state_message(&rpc, state).await?;
                    reread = self.apply(&mut tracker, state, Some(message));
                    if tracker.state == KlippyState::Ready && !tracker.chips_current {
                        reread = true;
                    }
                    next_poll = Instant::now() + poll_interval(&t, tracker.state);
                }
                Some(()) = self.refused.recv() => return Err(Failure::Unauthorized),
                () = self.publisher.status.closed() => return Err(Failure::Stop),
            }
        }
    }

    /// Klipper's message for `state`, asked of Moonraker only for states that have one. A
    /// Moonraker error answers with no message; a failed connection fails.
    async fn state_message(
        &self,
        rpc: &Rpc,
        state: KlippyState,
    ) -> Result<Option<String>, Failure> {
        if !state.has_message() {
            return Ok(None);
        }
        match rpc.call("printer.info", None, self.timings.request).await {
            Ok(info) => Ok(info
                .get("state_message")
                .and_then(Value::as_str)
                .map(str::to_owned)),
            Err(RpcError::Moonraker { .. }) => Ok(None),
            Err(e) => Err(self.failure(&e)),
        }
    }

    fn forward_gcode(&self, note: &Notification) {
        let lines = note.params.as_array().into_iter().flatten();
        for line in lines.filter_map(serde_json::Value::as_str) {
            self.publisher.publish_gcode(line.to_owned());
        }
    }

    /// Applies a Klipper notification and returns whether Klipper just became ready.
    fn on_notification(
        &self,
        tracker: &mut Tracker,
        note: &Notification,
        next_poll: &mut Instant,
    ) -> bool {
        if note.method == "notify_gcode_response" {
            self.forward_gcode(note);
            return false;
        }
        let Some(state) = notified_state(note).filter(|s| *s != tracker.state) else {
            return false;
        };
        let became_ready = self.apply(tracker, state, Some(None));
        *next_poll = Instant::now() + poll_interval(&self.timings, tracker.state);
        became_ready
    }

    /// Reads the chip list while still applying notifications, so a shutdown during a slow read
    /// shows at once. A slow or failing query marks the list not current and keeps the
    /// connection; a broken connection fails. The list counts as current only when Klipper's
    /// state did not change during the read. Returns whether Klipper is ready after such a
    /// change, so the list must be read again.
    async fn read_chips(
        &self,
        rpc: &Rpc,
        notes: &mut mpsc::UnboundedReceiver<Notification>,
        tracker: &mut Tracker,
        next_poll: &mut Instant,
    ) -> Result<bool, Failure> {
        let started = tracker.changes;
        let query = json!({"objects": {"configfile": ["config"]}});
        let call = rpc.call(
            "printer.objects.query",
            Some(query),
            self.timings.configfile,
        );
        tokio::pin!(call);
        let answer = loop {
            tokio::select! {
                answer = &mut call => break answer,
                note = notes.recv() => {
                    let Some(note) = note else {
                        return Err(self.lost(&RpcError::Closed));
                    };
                    self.on_notification(tracker, &note, next_poll);
                }
                () = self.publisher.status.closed() => return Err(Failure::Stop),
            }
        };
        let list = match answer {
            Ok(result) => parse_configfile(&result).ok(),
            Err(e) if e.is_unauthorized() => return Err(Failure::Unauthorized),
            Err(RpcError::Timeout | RpcError::Moonraker { .. }) => None,
            Err(e) => return Err(self.failure(&e)),
        };
        let unchanged = tracker.changes == started;
        tracker.chips_current = list.is_some() && unchanged;
        self.publisher.chips.send_modify(|snapshot| {
            if let Some(list) = list {
                snapshot.known = true;
                snapshot.list = list;
            }
            snapshot.current = tracker.chips_current;
        });
        Ok(!unchanged && tracker.state == KlippyState::Ready)
    }

    /// Publishes Klipper's new state and returns whether it just became ready.
    fn apply(
        &self,
        tracker: &mut Tracker,
        state: KlippyState,
        message: Option<Option<String>>,
    ) -> bool {
        let old = tracker.state;
        let message = message.unwrap_or_else(|| tracker.message.clone());
        if old == state && tracker.message == message {
            return false;
        }
        if old != state {
            let line = state_line("klipper", state.as_str(), message.as_deref());
            if matches!(state, KlippyState::Error | KlippyState::Shutdown) {
                tracing::warn!("{line}");
            } else {
                tracing::info!("{line}");
            }
        }
        if old == KlippyState::Ready && state != KlippyState::Ready {
            tracker.chips_current = false;
            self.publisher.chips.send_if_modified(|s| {
                let was_current = std::mem::replace(&mut s.current, false);
                s.reset_availability() | was_current
            });
        }
        if old != state {
            tracker.changes += 1;
        }
        tracker.state = state;
        tracker.message.clone_from(&message);
        self.publisher.status.send_modify(|s| {
            s.klippy.state = state;
            s.klippy.message = message;
        });
        state == KlippyState::Ready && old != KlippyState::Ready
    }

    fn publish_connected(&self, version: Option<String>) {
        self.log_moonraker(MoonrakerState::Connected, None);
        self.publisher.status.send_modify(|s| {
            s.moonraker.state = MoonrakerState::Connected;
            s.moonraker.message = None;
            s.moonraker.version = version;
        });
    }

    /// Publishes a failed connection: Moonraker's state, Klipper unknown, the chips not current.
    fn publish_failure(&self, failure: &Failure) {
        let (state, message) = match failure {
            Failure::Unreachable(message) => (MoonrakerState::Unreachable, message.clone()),
            Failure::Unauthorized => (
                MoonrakerState::Unauthorized,
                UNAUTHORIZED_MESSAGE.to_owned(),
            ),
            Failure::Stop => return,
        };
        self.log_moonraker(state, Some(&message));
        let klippy_was = self.publisher.status.borrow().klippy.state;
        if klippy_was != KlippyState::Unknown {
            tracing::info!(
                "{}",
                state_line("klipper", KlippyState::Unknown.as_str(), None)
            );
        }
        self.publisher.status.send_modify(|s| {
            s.moonraker.state = state;
            s.moonraker.message = Some(message);
            s.klippy.state = KlippyState::Unknown;
            s.klippy.message = None;
        });
        self.publisher.chips.send_if_modified(|s| {
            let was_current = std::mem::replace(&mut s.current, false);
            s.reset_availability() | was_current
        });
    }

    /// Logs a change of Moonraker's state, once per change.
    fn log_moonraker(&self, state: MoonrakerState, message: Option<&str>) {
        if self.publisher.status.borrow().moonraker.state != state {
            let line = state_line("moonraker", state.as_str(), message);
            if state == MoonrakerState::Connected {
                tracing::info!("{line}");
            } else {
                tracing::warn!("{line}");
            }
        }
    }

    /// Calls `method`, turning an error into the failure it means for the connection.
    async fn call(
        &self,
        rpc: &Rpc,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, Failure> {
        rpc.call(method, params, timeout)
            .await
            .map_err(|e| self.failure(&e))
    }

    fn failure(&self, e: &RpcError) -> Failure {
        if e.is_unauthorized() {
            Failure::Unauthorized
        } else {
            self.lost(e)
        }
    }

    fn lost(&self, e: &RpcError) -> Failure {
        Failure::Unreachable(format!(
            "the connection to Moonraker at {} failed: {e}",
            self.base_url
        ))
    }
}

fn klippy_state(info: &Value) -> KlippyState {
    info.get("klippy_state")
        .and_then(Value::as_str)
        .map_or(KlippyState::Unknown, KlippyState::parse)
}

/// The state a Klipper notification announces.
fn notified_state(note: &Notification) -> Option<KlippyState> {
    match note.method.as_str() {
        "notify_klippy_ready" => Some(KlippyState::Ready),
        "notify_klippy_shutdown" => Some(KlippyState::Shutdown),
        "notify_klippy_disconnected" => Some(KlippyState::Disconnected),
        _ => None,
    }
}

fn poll_interval(t: &Timings, state: KlippyState) -> Duration {
    if state == KlippyState::Ready {
        t.poll_ready
    } else {
        t.poll_not_ready
    }
}
