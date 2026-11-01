//! The bridge half of the fake Moonraker: connections on a `/klippysocket` path speak Klipper's
//! webhooks protocol, not JSON-RPC. A test scripts how a subscription is answered and pushes
//! batches to the subscribed sensors.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

/// How the fake answers one subscribe request.
#[derive(Clone)]
pub enum Subscribe {
    /// Accepts, with the header Klipper sends.
    Ok,
    /// Answers with a Klipper error and the given message.
    Error(String),
    /// Never answers.
    Silent,
    /// Closes the socket.
    Close,
}

type Handler = Box<dyn FnMut(&Value, usize) -> Subscribe + Send>;

enum Out {
    Text(String),
    Close,
}

struct Sub {
    sensor: String,
    out: mpsc::UnboundedSender<Out>,
    open: bool,
}

struct State {
    handler: Handler,
    subs: Vec<Sub>,
    requests: Vec<Value>,
    tokens: Vec<String>,
    per_sensor: std::collections::HashMap<String, usize>,
}

/// The scriptable bridge. Clones share it.
#[derive(Clone)]
pub struct FakeBridge {
    state: Arc<Mutex<State>>,
}

impl Default for FakeBridge {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                handler: Box::new(|_, _| Subscribe::Ok),
                subs: Vec::new(),
                requests: Vec::new(),
                tokens: Vec::new(),
                per_sensor: Default::default(),
            })),
        }
    }
}

impl FakeBridge {
    /// Picks the answer to each subscribe request from its frame and the number of earlier
    /// subscribes for the same sensor.
    pub fn on_subscribe(&self, handler: impl FnMut(&Value, usize) -> Subscribe + Send + 'static) {
        self.state.lock().unwrap().handler = Box::new(handler);
    }

    /// Answers every subscribe for `sensor` with `answer`, and the rest as usual.
    pub fn answer_for(&self, sensor: &str, answer: Subscribe) {
        let sensor = sensor.to_owned();
        self.on_subscribe(move |frame, _| {
            if frame["params"]["sensor"].as_str() == Some(sensor.as_str()) {
                answer.clone()
            } else {
                Subscribe::Ok
            }
        });
    }

    /// Sends a batch frame to every open subscription on `sensor`. `rows` are `[t, x, y, z]`.
    pub fn push(&self, sensor: &str, rows: &[[f64; 4]], counters: Option<(u64, u64)>) {
        let mut params = json!({"data": rows});
        if let Some((errors, overflows)) = counters {
            params["errors"] = json!(errors);
            params["overflows"] = json!(overflows);
        }
        self.push_frame(sensor, &json!({"params": params}));
    }

    /// Sends any frame to every open subscription on `sensor`.
    pub fn push_frame(&self, sensor: &str, frame: &Value) {
        for sub in &self.state.lock().unwrap().subs {
            if sub.open && sub.sensor == sensor {
                let _ = sub.out.send(Out::Text(frame.to_string()));
            }
        }
    }

    /// Closes every open subscription on `sensor`.
    pub fn drop_sensor(&self, sensor: &str) {
        for sub in &self.state.lock().unwrap().subs {
            if sub.sensor == sensor {
                let _ = sub.out.send(Out::Close);
            }
        }
    }

    /// Closes every open subscription.
    pub fn drop_all(&self) {
        for sub in &self.state.lock().unwrap().subs {
            let _ = sub.out.send(Out::Close);
        }
    }

    /// How many subscriptions are open on `sensor` right now.
    pub fn open_on(&self, sensor: &str) -> usize {
        self.state
            .lock()
            .unwrap()
            .subs
            .iter()
            .filter(|s| s.open && s.sensor == sensor)
            .count()
    }

    /// How many subscriptions are open on any sensor.
    pub fn open_total(&self) -> usize {
        self.state
            .lock()
            .unwrap()
            .subs
            .iter()
            .filter(|s| s.open)
            .count()
    }

    /// How many subscribe requests were received, open or not.
    pub fn subscribes(&self) -> usize {
        self.state.lock().unwrap().requests.len()
    }

    /// The subscribe frames received, in order.
    pub fn requests(&self) -> Vec<Value> {
        self.state.lock().unwrap().requests.clone()
    }

    /// The token query values connections arrived with.
    pub fn tokens(&self) -> Vec<String> {
        self.state.lock().unwrap().tokens.clone()
    }
}

/// Serves one bridge connection until it closes.
pub async fn serve(
    socket: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    bridge: FakeBridge,
    token: String,
) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Out>();
    bridge.state.lock().unwrap().tokens.push(token);
    let mut index = None;
    loop {
        tokio::select! {
            out = out_rx.recv() => match out {
                Some(Out::Text(text)) => {
                    if sink.send(Message::text(text)).await.is_err() { break; }
                }
                Some(Out::Close) | None => {
                    let _ = sink.close().await;
                    break;
                }
            },
            frame = stream.next() => {
                let Some(Ok(frame)) = frame else { break };
                let Message::Text(text) = frame else {
                    if matches!(frame, Message::Close(_)) { break; }
                    continue;
                };
                let Ok(frame) = serde_json::from_str::<Value>(&text) else { continue };
                let sensor = frame["params"]["sensor"].as_str().unwrap_or("").to_owned();
                let answer = {
                    let mut s = bridge.state.lock().unwrap();
                    s.requests.push(frame.clone());
                    let n = s.per_sensor.entry(sensor.clone()).or_insert(0);
                    let before = *n;
                    *n += 1;
                    (s.handler)(&frame, before)
                };
                match answer {
                    Subscribe::Ok => {
                        {
                            let mut s = bridge.state.lock().unwrap();
                            s.subs.push(Sub { sensor, out: out_tx.clone(), open: true });
                            index = Some(s.subs.len() - 1);
                        }
                        let reply = json!({"id": frame["id"], "result": {"header": ["time", "x_acceleration", "y_acceleration", "z_acceleration"]}});
                        if sink.send(Message::text(reply.to_string())).await.is_err() { break; }
                    }
                    Subscribe::Error(message) => {
                        let reply = json!({"id": frame["id"], "error": {"error": "WebRequestError", "message": message}});
                        if sink.send(Message::text(reply.to_string())).await.is_err() { break; }
                    }
                    Subscribe::Silent => {}
                    Subscribe::Close => {
                        let _ = sink.close().await;
                        break;
                    }
                }
            }
        }
    }
    if let Some(i) = index {
        bridge.state.lock().unwrap().subs[i].open = false;
    }
}
