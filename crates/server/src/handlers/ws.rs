//! WebSocket handler for streaming real-time metrics to frontend clients.

use crate::state::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use tracing::debug;

/// Upgrade HTTP request to WebSocket telemetry subscription connection (`GET /ws`).
pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Manage connected WebSocket stream, broadcasting updates from `AppState::tx`.
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    // Send initial device list on connection
    let initial_devices = state.get_devices().await;
    let initial_event = crate::state::WsEvent::List(initial_devices);
    if let Ok(json) = serde_json::to_string(&initial_event) {
        if sender.send(Message::Text(json)).await.is_err() {
            return;
        }
    }

    // Task to forward broadcast events to this client
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&event) {
                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Task to consume incoming control messages (e.g. ping/close) from client
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Close(_) = msg {
                break;
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => debug!("WebSocket send loop terminated"),
        _ = &mut recv_task => debug!("WebSocket recv loop terminated"),
    }

    send_task.abort();
    recv_task.abort();
    debug!("WebSocket connection closed");
}
