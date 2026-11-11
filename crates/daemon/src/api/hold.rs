//! `GET /api/hold`: the control websocket of a held excitation.
//!
//! The browser sends `start`, then `set` and `stop`, and a `ping` every 0.5 s. The service ends
//! the hold after [`HoldSocketTimings::keepalive_silence`] without any message and at once when
//! the socket closes, so the printer stops when the tab goes.

use std::time::Duration;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::unbounded_channel;
use tokio::time::Instant;

use super::ApiState;
use crate::capture::CheckFailure;
use crate::hold::pieces::HoldAxis;
use crate::hold::runner::{
    Command, EndReason, HoldDeps, HoldEnded, HoldEvent, HoldRequest, HoldState, HoldTimings,
    Refusal, begin,
};
use crate::hold::settings::Restore;

/// How long a socket may wait before it sends `start`.
const START_WAIT: Duration = Duration::from_secs(30);
/// How long one write to the browser may take before the hold is ended.
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// The control socket's limits. Clones share nothing.
#[derive(Debug, Clone)]
pub struct HoldSocketTimings {
    /// How long the browser may send nothing before the hold ends.
    pub keepalive_silence: Duration,
    /// The hold's own limits.
    pub hold: HoldTimings,
}

impl Default for HoldSocketTimings {
    fn default() -> Self {
        Self {
            keepalive_silence: Duration::from_millis(1500),
            hold: HoldTimings::default(),
        }
    }
}

/// A message from the browser.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HoldClientMessage {
    /// Starts the hold. Only the first one counts.
    Start(HoldRequest),
    /// Changes the frequency or the axis of a running hold.
    Set {
        /// The new frequency, in Hz.
        #[serde(default)]
        frequency: Option<f64>,
        /// The new axis.
        #[serde(default)]
        axis: Option<HoldAxis>,
    },
    /// Stops gracefully.
    Stop,
    /// Says the browser is still there.
    Ping,
}

/// A message to the browser.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HoldServerMessage {
    /// The hold may not start. Every failed check is listed. The socket closes after it.
    Refused {
        /// The failed checks.
        failures: Vec<CheckFailure>,
    },
    /// The hold has the slot. The id is what the emergency-stop endpoint takes.
    Started {
        /// The hold's id.
        id: String,
    },
    /// The hold's state, about every 0.25 s.
    State(HoldState),
    /// A change was not applied.
    Rejected {
        /// What was wrong with the change.
        message: String,
    },
    /// The hold ended. The socket closes after it.
    Ended(HoldEnded),
}

pub(super) async fn hold(State(state): State<ApiState>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| serve(socket, state))
}

async fn write(socket: &mut WebSocket, message: &HoldServerMessage) -> bool {
    let text = serde_json::to_string(message).expect("hold message serializes");
    tokio::time::timeout(WRITE_TIMEOUT, socket.send(Message::text(text)))
        .await
        .is_ok_and(|sent| sent.is_ok())
}

/// Sends a close frame so the browser sees the socket end cleanly.
async fn close(socket: &mut WebSocket) {
    let _ = tokio::time::timeout(WRITE_TIMEOUT, socket.send(Message::Close(None))).await;
}

fn to_message(event: HoldEvent) -> HoldServerMessage {
    match event {
        HoldEvent::Started { id } => HoldServerMessage::Started { id },
        HoldEvent::State(state) => HoldServerMessage::State(state),
        HoldEvent::Rejected { message } => HoldServerMessage::Rejected { message },
    }
}

/// Waits for the `start` message. Anything else is ignored.
async fn wait_for_start(socket: &mut WebSocket) -> Option<HoldRequest> {
    let wait = async {
        loop {
            match socket.recv().await {
                None | Some(Err(_) | Ok(Message::Close(_))) => return None,
                Some(Ok(Message::Text(text))) => {
                    if let Ok(HoldClientMessage::Start(request)) = serde_json::from_str(&text) {
                        return Some(request);
                    }
                }
                Some(Ok(_)) => {}
            }
        }
    };
    tokio::time::timeout(START_WAIT, wait).await.ok().flatten()
}

async fn serve(mut socket: WebSocket, state: ApiState) {
    let timings = state.capture.hold.clone();
    let Some(request) = wait_for_start(&mut socket).await else {
        return;
    };
    let deps = HoldDeps {
        moonraker: &state.moonraker,
        slot: &state.capture.slot,
        timings: timings.hold.clone(),
    };
    let hold = match begin(&deps, request).await {
        Ok(hold) => hold,
        Err(refusal) => {
            let failures = match refusal {
                Refusal::Checks(failures) => failures,
                Refusal::Invalid(message) => vec![CheckFailure::new("request", message)],
            };
            write(&mut socket, &HoldServerMessage::Refused { failures }).await;
            close(&mut socket).await;
            return;
        }
    };
    let (commands, command_rx) = unbounded_channel();
    let (event_tx, mut events) = unbounded_channel();
    let mut run = tokio::spawn(hold.run(command_rx, event_tx));
    let silence = tokio::time::sleep(timings.keepalive_silence);
    tokio::pin!(silence);
    let mut lost = false;
    let ended = loop {
        tokio::select! {
            Some(event) = events.recv() => {
                if !write(&mut socket, &to_message(event)).await && !lost {
                    lost = true;
                    let _ = commands.send(Command::ConnectionLost);
                }
            }
            incoming = socket.recv(), if !lost => match incoming {
                None | Some(Err(_) | Ok(Message::Close(_))) => {
                    lost = true;
                    let _ = commands.send(Command::ConnectionLost);
                }
                Some(Ok(Message::Text(text))) => {
                    silence.as_mut().reset(Instant::now() + timings.keepalive_silence);
                    match serde_json::from_str::<HoldClientMessage>(&text) {
                        Ok(HoldClientMessage::Set { frequency, axis }) => {
                            if let Some(f) = frequency {
                                let _ = commands.send(Command::SetFrequency(f));
                            }
                            if let Some(a) = axis {
                                let _ = commands.send(Command::SetAxis(a));
                            }
                        }
                        Ok(HoldClientMessage::Stop) => {
                            let _ = commands.send(Command::Stop);
                        }
                        Ok(HoldClientMessage::Start(_) | HoldClientMessage::Ping) | Err(_) => {}
                    }
                }
                Some(Ok(_)) => {}
            },
            () = &mut silence, if !lost => {
                lost = true;
                let _ = commands.send(Command::ConnectionLost);
            }
            result = &mut run => {
                break result.unwrap_or_else(|e| HoldEnded {
                    reason: EndReason::Failed { message: format!("the hold task failed: {e}") },
                    restore: Restore::NotNeeded,
                });
            }
        }
    };
    while let Ok(event) = events.try_recv() {
        write(&mut socket, &to_message(event)).await;
    }
    write(&mut socket, &HoldServerMessage::Ended(ended)).await;
    close(&mut socket).await;
}
