//! One chip's accelerometer dump over Moonraker's `/klippysocket` bridge.
//!
//! The bridge relays Klipper's own webhooks socket, which is not JSON-RPC 2.0, so
//! [`super::rpc::Rpc`] cannot carry it. A [`Bridge`] connects, subscribes to one chip's dump
//! endpoint and hands out the batches Klipper pushes. Dropping it closes the socket, which ends
//! Klipper's subscription.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};
use wobblescope_core::RawSamples;

use super::tls::Tls;

/// The request id the one subscribe call carries.
const SUBSCRIBE_ID: u64 = 1;

/// One batch of samples as Klipper sent it.
#[derive(Debug, Clone, PartialEq)]
pub struct Batch {
    /// The samples, in the order they arrived.
    pub samples: RawSamples,
    /// Klipper's running count of read errors, when the chip type reports one.
    pub errors: Option<u64>,
    /// Klipper's running count of buffer overflows, when the chip type reports one.
    pub overflows: Option<u64>,
}

/// Why a bridge could not be opened.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BridgeError {
    /// The websocket could not be opened.
    #[error("cannot open the bridge: {0}")]
    Connect(String),
    /// Klipper refused the subscription, with its own message.
    #[error("{0}")]
    Subscribe(String),
    /// Nothing answered the subscription in time.
    #[error("the subscription was not answered in time")]
    Timeout,
    /// The bridge sent something that is not what the dump endpoint answers with.
    #[error("the bridge sent something unexpected: {0}")]
    Protocol(String),
}

/// What the bridge delivered next.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A batch of samples.
    Batch(Batch),
    /// The stream ended, with the reason. Nothing follows.
    Closed(String),
}

/// An open subscription to one chip. Dropping it closes the socket.
pub struct Bridge {
    events: mpsc::UnboundedReceiver<Event>,
    task: JoinHandle<()>,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Bridge {
    /// Connects to `url`, verifying a `wss` server as `tls` says, subscribes to `endpoint` (with
    /// `sensor` when the chip has one) and waits up to `timeout` for Klipper's answer.
    ///
    /// # Errors
    /// A [`BridgeError`]: the socket would not open, Klipper refused the subscription, or
    /// nothing answered in time.
    pub async fn open(
        url: &str,
        endpoint: &str,
        sensor: Option<&str>,
        timeout: Duration,
        tls: &Tls,
    ) -> Result<Self, BridgeError> {
        let deadline = tokio::time::Instant::now() + timeout;
        let connect = tokio::time::timeout_at(deadline, super::tls::connect(url, tls));
        let mut socket = match connect.await {
            Err(_) => return Err(BridgeError::Timeout),
            Ok(Err(WsError::Http(response))) => {
                return Err(BridgeError::Connect(format!(
                    "Moonraker refused the bridge with HTTP {}",
                    response.status().as_u16()
                )));
            }
            Ok(Err(e)) => return Err(BridgeError::Connect(e.to_string())),
            Ok(Ok(socket)) => socket,
        };
        let mut params = json!({"response_template": {}});
        if let Some(sensor) = sensor {
            params["sensor"] = json!(sensor);
        }
        let request = json!({"id": SUBSCRIBE_ID, "method": endpoint, "params": params});
        socket
            .send(Message::text(request.to_string()))
            .await
            .map_err(|e| BridgeError::Connect(e.to_string()))?;

        let (tx, events) = mpsc::unbounded_channel();
        // A batch that races ahead of the subscribe reply is kept and delivered first.
        let answered = tokio::time::timeout_at(deadline, async {
            loop {
                let frame = match socket.next().await {
                    None | Some(Ok(Message::Close(_))) => {
                        return Err(BridgeError::Connect(
                            "the bridge closed before it answered".to_owned(),
                        ));
                    }
                    Some(Err(e)) => return Err(BridgeError::Connect(e.to_string())),
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(_)) => continue,
                };
                let frame: Value = serde_json::from_str(&frame)
                    .map_err(|_| BridgeError::Protocol("a frame that is not JSON".to_owned()))?;
                if frame.get("id").and_then(Value::as_u64) == Some(SUBSCRIBE_ID) {
                    if let Some(error) = frame.get("error") {
                        return Err(BridgeError::Subscribe(error_message(error)));
                    }
                    return Ok(());
                }
                if let Some(batch) = parse_batch(&frame)? {
                    let _ = tx.send(Event::Batch(batch));
                }
            }
        })
        .await;
        match answered {
            Err(_) => return Err(BridgeError::Timeout),
            Ok(Err(e)) => return Err(e),
            Ok(Ok(())) => {}
        }
        let task = tokio::spawn(pump(socket, tx));
        Ok(Self { events, task })
    }

    /// The next event. After [`Event::Closed`] it keeps answering `Closed`.
    pub async fn recv(&mut self) -> Event {
        self.events
            .recv()
            .await
            .unwrap_or_else(|| Event::Closed("the bridge connection closed".to_owned()))
    }
}

/// Reads the socket until it ends, sending each batch and finally the reason it ended.
async fn pump(
    mut socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    tx: mpsc::UnboundedSender<Event>,
) {
    let reason = loop {
        match socket.next().await {
            None | Some(Ok(Message::Close(_))) => {
                break "the bridge connection closed".to_owned();
            }
            Some(Err(e)) => break format!("the bridge connection failed: {e}"),
            Some(Ok(Message::Text(text))) => {
                let batch = serde_json::from_str::<Value>(&text)
                    .map_err(|_| BridgeError::Protocol("a frame that is not JSON".to_owned()))
                    .and_then(|frame| parse_batch(&frame));
                match batch {
                    Ok(Some(batch)) => {
                        if tx.send(Event::Batch(batch)).is_err() {
                            return;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => break e.to_string(),
                }
            }
            Some(Ok(_)) => {}
        }
    };
    let _ = tx.send(Event::Closed(reason));
}

/// Klipper's message from an error object, or the whole object when it has none.
fn error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .map_or_else(|| error.to_string(), str::to_owned)
}

/// Parses a frame that carries a batch. A frame without `params` is not a batch.
fn parse_batch(frame: &Value) -> Result<Option<Batch>, BridgeError> {
    let Some(params) = frame.get("params") else {
        return Ok(None);
    };
    let protocol = |what: &str| BridgeError::Protocol(what.to_owned());
    let rows = params
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| protocol("a batch without data"))?;
    let mut samples = RawSamples::default();
    for row in rows {
        let row = row
            .as_array()
            .filter(|r| r.len() == 4)
            .ok_or_else(|| protocol("a sample that is not [t, x, y, z]"))?;
        let mut v = [0.0; 4];
        for (slot, value) in v.iter_mut().zip(row) {
            *slot = value
                .as_f64()
                .ok_or_else(|| protocol("a sample value that is not a number"))?;
        }
        samples.t.push(v[0]);
        samples.x.push(v[1]);
        samples.y.push(v[2]);
        samples.z.push(v[3]);
    }
    Ok(Some(Batch {
        samples,
        errors: params.get("errors").and_then(Value::as_u64),
        overflows: params.get("overflows").and_then(Value::as_u64),
    }))
}
