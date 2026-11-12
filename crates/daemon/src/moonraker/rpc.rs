//! JSON-RPC over Moonraker's websocket: calls with a timeout, a stream of notifications, and
//! notice of a closed connection.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use super::tls::Tls;

/// JSON-RPC code Moonraker uses for invalid params and for failed authentication.
const INVALID_PARAMS: i64 = -32602;

/// Why a call or the connection failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RpcError {
    /// The websocket could not be opened, or Moonraker refused the upgrade.
    #[error("{0}")]
    Connect(String),
    /// The connection is closed.
    #[error("the connection to Moonraker closed")]
    Closed,
    /// The socket failed while open.
    #[error("connection to Moonraker failed: {0}")]
    Io(String),
    /// Nothing answered in time.
    #[error("the request timed out")]
    Timeout,
    /// Moonraker sent a frame that is not JSON-RPC.
    #[error("Moonraker sent something that is not JSON-RPC: {0}")]
    Protocol(String),
    /// Moonraker answered with a JSON-RPC error.
    #[error("Moonraker answered with error {code}: {message}")]
    Moonraker {
        /// The JSON-RPC error code.
        code: i64,
        /// Moonraker's message.
        message: String,
    },
}

impl RpcError {
    /// Whether Moonraker refused the client for lack of authentication. `-32602` is also its
    /// code for invalid params, so the message decides.
    #[must_use]
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Self::Moonraker { code, message }
            if *code == INVALID_PARAMS && matches!(message.as_str(), "Unauthorized" | "Invalid API Key"))
    }
}

/// A frame Moonraker pushed without being asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    /// The notification's method, for example `notify_klippy_ready`.
    pub method: String,
    /// Its params, or null.
    pub params: Value,
}

type Reply = oneshot::Sender<Result<Value, RpcError>>;

struct Call {
    id: u64,
    frame: String,
    reply: Reply,
}

/// A connection to Moonraker. Clones share it.
#[derive(Clone)]
pub struct Rpc {
    calls: mpsc::UnboundedSender<Call>,
    next_id: Arc<AtomicU64>,
}

impl Rpc {
    /// Opens the websocket at `url`, verifying a `wss` server as `tls` says.
    ///
    /// Returns the client and the stream of notifications, which ends when the connection does.
    ///
    /// # Errors
    /// [`RpcError::Timeout`] when the upgrade takes longer than `timeout`, and
    /// [`RpcError::Connect`] when it fails or is refused.
    pub async fn connect(
        url: &str,
        timeout: Duration,
        tls: &Tls,
    ) -> Result<(Self, mpsc::UnboundedReceiver<Notification>), RpcError> {
        let socket = match tokio::time::timeout(timeout, super::tls::connect(url, tls)).await {
            Err(_) => return Err(RpcError::Timeout),
            Ok(Err(WsError::Http(response))) => {
                return Err(RpcError::Connect(format!(
                    "Moonraker refused the websocket with HTTP {}",
                    response.status().as_u16()
                )));
            }
            Ok(Err(e)) => return Err(RpcError::Connect(e.to_string())),
            Ok(Ok(socket)) => socket,
        };
        let (calls, call_rx) = mpsc::unbounded_channel();
        let (notes, note_rx) = mpsc::unbounded_channel();
        tokio::spawn(run(socket, call_rx, notes));
        let rpc = Self {
            calls,
            next_id: Arc::new(AtomicU64::new(1)),
        };
        Ok((rpc, note_rx))
    }

    /// Calls `method` with `params` and waits up to `timeout` for the result.
    ///
    /// # Errors
    /// An [`RpcError`]: the connection closed, the call timed out, or Moonraker answered with an
    /// error.
    pub async fn call(
        &self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut frame = json!({"jsonrpc": "2.0", "method": method, "id": id});
        if let Some(params) = params {
            frame["params"] = params;
        }
        let (reply, answer) = oneshot::channel();
        let call = Call {
            id,
            frame: frame.to_string(),
            reply,
        };
        self.calls.send(call).map_err(|_| RpcError::Closed)?;
        match tokio::time::timeout(timeout, answer).await {
            Err(_) => Err(RpcError::Timeout),
            Ok(Err(_)) => Err(RpcError::Closed),
            Ok(Ok(result)) => result,
        }
    }
}

/// Owns the socket: sends calls, routes each response to its call and each notification to the
/// stream. On the way out it fails every call still waiting.
async fn run(
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut calls: mpsc::UnboundedReceiver<Call>,
    notes: mpsc::UnboundedSender<Notification>,
) {
    let (mut sink, mut stream) = socket.split();
    let mut pending: HashMap<u64, Reply> = HashMap::new();
    let end = loop {
        tokio::select! {
            call = calls.recv() => {
                let Some(call) = call else { break RpcError::Closed };
                if let Err(e) = sink.send(Message::text(call.frame)).await {
                    let _ = call.reply.send(Err(RpcError::Io(e.to_string())));
                    break RpcError::Io(e.to_string());
                }
                pending.insert(call.id, call.reply);
            }
            frame = stream.next() => match frame {
                None | Some(Ok(Message::Close(_))) => break RpcError::Closed,
                Some(Err(e)) => break RpcError::Io(e.to_string()),
                Some(Ok(Message::Text(text))) => {
                    if let Err(e) = route(&text, &mut pending, &notes) {
                        break e;
                    }
                }
                Some(Ok(_)) => {}
            },
        }
    };
    for (_, reply) in pending.drain() {
        let _ = reply.send(Err(end.clone()));
    }
}

/// Delivers one text frame to its call or to the notification stream.
fn route(
    text: &str,
    pending: &mut HashMap<u64, Reply>,
    notes: &mpsc::UnboundedSender<Notification>,
) -> Result<(), RpcError> {
    let protocol = |what: &str| RpcError::Protocol(what.to_owned());
    let frame: Value =
        serde_json::from_str(text).map_err(|_| protocol("a frame that is not JSON"))?;
    let Some(frame) = frame
        .as_object()
        .filter(|f| f.get("jsonrpc") == Some(&json!("2.0")))
    else {
        return Err(protocol("a frame without jsonrpc 2.0"));
    };
    if let Some(id) = frame.get("id").and_then(Value::as_u64) {
        let outcome = match (frame.get("result"), frame.get("error")) {
            (_, Some(error)) => Err(RpcError::Moonraker {
                code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            }),
            (Some(result), None) => Ok(result.clone()),
            (None, None) => return Err(protocol("a response with neither result nor error")),
        };
        if let Some(reply) = pending.remove(&id) {
            let _ = reply.send(outcome);
        }
        return Ok(());
    }
    let Some(method) = frame.get("method").and_then(Value::as_str) else {
        return Err(protocol("a frame with neither id nor method"));
    };
    let _ = notes.send(Notification {
        method: method.to_owned(),
        params: frame.get("params").cloned().unwrap_or(Value::Null),
    });
    Ok(())
}
