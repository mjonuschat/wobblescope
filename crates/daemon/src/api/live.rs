//! `GET /api/live`: a websocket that sends the running test's state and the last finished test.
//! It carries no samples. The first message is the current snapshot, every change follows, and a
//! subscriber that falls behind is sent a fresh snapshot instead of being closed.

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use tokio::sync::broadcast::error::RecvError;

use super::ApiState;
use crate::capture::live::LiveSnapshot;

pub(super) async fn live(State(state): State<ApiState>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| serve(socket, state))
}

fn frame(snapshot: &LiveSnapshot) -> Message {
    Message::text(serde_json::to_string(snapshot).unwrap_or_else(|_| "{}".to_owned()))
}

async fn serve(mut socket: WebSocket, state: ApiState) {
    let hub = state.capture.hub.clone();
    let (first, mut changes) = hub.subscribe();
    if socket.send(frame(&first)).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            change = changes.recv() => {
                let snapshot = match change {
                    Ok(snapshot) => snapshot,
                    Err(RecvError::Lagged(_)) => hub.snapshot(),
                    Err(RecvError::Closed) => return,
                };
                if socket.send(frame(&snapshot)).await.is_err() {
                    return;
                }
            }
            incoming = socket.recv() => {
                // The client sends nothing; a close or an error ends the stream.
                match incoming {
                    None | Some(Err(_) | Ok(Message::Close(_))) => return,
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}
