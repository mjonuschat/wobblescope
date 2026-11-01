//! A scriptable Moonraker: a websocket server on a local port that answers JSON-RPC calls from
//! per-method scripts, pushes notifications and can misbehave on demand.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

/// What the fake does with a call.
#[derive(Clone)]
pub enum Script {
    /// Answers with this result.
    Result(Value),
    /// Answers with a JSON-RPC error.
    Error(i64, &'static str),
    /// Answers after a delay.
    After(Duration, Box<Script>),
    /// Never answers.
    Silent,
    /// Closes the socket.
    Close,
}

impl Script {
    pub fn error(code: i64, message: &'static str) -> Self {
        Self::Error(code, message)
    }

    pub fn after(delay: Duration, then: Script) -> Self {
        Self::After(delay, Box::new(then))
    }
}

/// Picks the script for one call from its params and how many calls to this method came before.
type Handler = Box<dyn FnMut(&Value, usize) -> Script + Send>;

/// One call the fake received.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub params: Value,
    /// The whole frame, for checking `jsonrpc` and `id`.
    pub frame: Value,
    /// Which accepted connection sent it, from 1.
    pub connection: usize,
}

enum Out {
    Text(String),
    Close,
}

#[derive(Default)]
struct State {
    handlers: HashMap<String, Handler>,
    calls_per_method: HashMap<String, usize>,
    recorded: Vec<Recorded>,
    outbound: Vec<mpsc::UnboundedSender<Out>>,
    attempts: Vec<Instant>,
    connections: usize,
    accepting: bool,
    upgrade_status: Option<u16>,
}

/// The fake. Dropping it stops the server.
pub struct FakeMoonraker {
    addr: std::net::SocketAddr,
    state: Arc<Mutex<State>>,
    accept: tokio::task::JoinHandle<()>,
}

impl Drop for FakeMoonraker {
    fn drop(&mut self) {
        self.accept.abort();
    }
}

impl FakeMoonraker {
    /// Starts a fake that accepts connections and answers every method with an empty object until
    /// scripted.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let state = Arc::new(Mutex::new(State {
            accepting: true,
            ..State::default()
        }));
        let accept = tokio::spawn(accept_loop(listener, state.clone()));
        Self {
            addr,
            state,
            accept,
        }
    }

    /// The websocket URL.
    pub fn ws_url(&self) -> String {
        format!("ws://{}/websocket", self.addr)
    }

    /// The base URL a config would hold.
    pub fn http_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Answers every call to `method` with `script`.
    pub fn on(&self, method: &str, script: Script) {
        self.on_fn(method, move |_, _| script.clone());
    }

    /// Answers each call to `method` with what `handler` picks from its params and the number of
    /// earlier calls to that method.
    pub fn on_fn(
        &self,
        method: &str,
        handler: impl FnMut(&Value, usize) -> Script + Send + 'static,
    ) {
        self.state
            .lock()
            .unwrap()
            .handlers
            .insert(method.to_owned(), Box::new(handler));
    }

    /// Pushes a notification to every open connection.
    pub fn push(&self, method: &str, params: Value) {
        let frame = json!({"jsonrpc": "2.0", "method": method, "params": params});
        self.push_raw(&frame.to_string());
    }

    /// Sends `text` as one frame to every open connection.
    pub fn push_raw(&self, text: &str) {
        for out in &self.state.lock().unwrap().outbound {
            let _ = out.send(Out::Text(text.to_owned()));
        }
    }

    /// Closes every open connection.
    pub fn close_connections(&self) {
        let mut state = self.state.lock().unwrap();
        for out in state.outbound.drain(..) {
            let _ = out.send(Out::Close);
        }
    }

    /// While false, a connection is dropped before the upgrade.
    pub fn set_accepting(&self, accepting: bool) {
        self.state.lock().unwrap().accepting = accepting;
    }

    /// Answers the upgrade with this HTTP status instead of switching protocols.
    pub fn refuse_upgrade_with(&self, status: Option<u16>) {
        self.state.lock().unwrap().upgrade_status = status;
    }

    /// Every call received so far, in order.
    pub fn requests(&self) -> Vec<Recorded> {
        self.state.lock().unwrap().recorded.clone()
    }

    /// The calls received for `method`.
    pub fn requests_for(&self, method: &str) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|r| r.method == method)
            .collect()
    }

    /// When each TCP connection arrived, accepted or not.
    pub fn attempts(&self) -> Vec<Instant> {
        self.state.lock().unwrap().attempts.clone()
    }

    /// How many websocket connections were established.
    pub fn connections(&self) -> usize {
        self.state.lock().unwrap().connections
    }
}

async fn accept_loop(listener: TcpListener, state: Arc<Mutex<State>>) {
    loop {
        let Ok((tcp, _)) = listener.accept().await else {
            return;
        };
        let (accepting, status) = {
            let mut s = state.lock().unwrap();
            s.attempts.push(Instant::now());
            (s.accepting, s.upgrade_status)
        };
        if !accepting {
            drop(tcp);
            continue;
        }
        tokio::spawn(connection(tcp, state.clone(), status));
    }
}

/// Switches protocols, or answers the upgrade with `status` when the test set one.
#[allow(clippy::result_large_err)]
fn upgrade(status: Option<u16>, response: Response) -> Result<Response, ErrorResponse> {
    match status {
        Some(code) => Err(tokio_tungstenite::tungstenite::http::Response::builder()
            .status(code)
            .body(None)
            .unwrap()),
        None => Ok(response),
    }
}

#[allow(clippy::result_large_err)]
async fn connection(tcp: tokio::net::TcpStream, state: Arc<Mutex<State>>, status: Option<u16>) {
    let Ok(socket) =
        tokio_tungstenite::accept_hdr_async(tcp, move |_: &Request, response: Response| {
            upgrade(status, response)
        })
        .await
    else {
        return;
    };
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Out>();
    let number = {
        let mut s = state.lock().unwrap();
        s.connections += 1;
        s.outbound.push(out_tx.clone());
        s.connections
    };
    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            out = out_rx.recv() => match out {
                Some(Out::Text(text)) => {
                    if sink.send(Message::text(text)).await.is_err() { return; }
                }
                Some(Out::Close) | None => {
                    let _ = sink.close().await;
                    return;
                }
            },
            frame = stream.next() => {
                let Some(Ok(Message::Text(text))) = frame else {
                    if matches!(frame, Some(Ok(_))) { continue; }
                    return;
                };
                let Ok(frame) = serde_json::from_str::<Value>(&text) else { continue };
                let method = frame["method"].as_str().unwrap_or_default().to_owned();
                let params = frame.get("params").cloned().unwrap_or(Value::Null);
                let script = {
                    let mut s = state.lock().unwrap();
                    s.recorded.push(Recorded {
                        method: method.clone(),
                        params: params.clone(),
                        frame: frame.clone(),
                        connection: number,
                    });
                    let count = s.calls_per_method.entry(method.clone()).or_insert(0);
                    let before = *count;
                    *count += 1;
                    match s.handlers.get_mut(&method) {
                        Some(handler) => handler(&params, before),
                        None => Script::Result(json!({})),
                    }
                };
                reply(&out_tx, frame["id"].clone(), script);
            }
        }
    }
}

/// Sends the script's outcome on `out`, after its delay when it has one.
fn reply(out: &mpsc::UnboundedSender<Out>, id: Value, script: Script) {
    match script {
        Script::Result(result) => {
            let frame = json!({"jsonrpc": "2.0", "result": result, "id": id});
            let _ = out.send(Out::Text(frame.to_string()));
        }
        Script::Error(code, message) => {
            let frame =
                json!({"jsonrpc": "2.0", "error": {"code": code, "message": message}, "id": id});
            let _ = out.send(Out::Text(frame.to_string()));
        }
        Script::After(delay, then) => {
            let out = out.clone();
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                reply(&out, id, *then);
            });
        }
        Script::Silent => {}
        Script::Close => {
            let _ = out.send(Out::Close);
        }
    }
}

/// Polls `condition` until it holds, panicking with `what` after `deadline`.
pub async fn eventually(what: &str, deadline: Duration, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(start.elapsed() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
