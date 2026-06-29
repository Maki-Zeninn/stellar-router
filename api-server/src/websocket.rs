use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::time::Duration;
use std::collections::HashSet;
use tracing::{error, info, warn};

use crate::{
    state::{AppState, MAX_SUBSCRIPTIONS_PER_CONNECTION},
    types::{SubscribeMessage, TransactionStatusEvent},
};

const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// WebSocket upgrade handler
pub async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    if !state.try_acquire_ws_connection() {
        return ws
            .on_upgrade(|mut socket| async move {
                let msg = json!({"error": "connection limit reached"}).to_string();
                let _ = socket.send(Message::Text(msg)).await;
                let _ = socket.close().await;
            })
            .into_response();
    }
    ws.on_upgrade(|socket| handle_socket(socket, state))
        .into_response()
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    info!("WebSocket client connected");

    let mut subscriptions: Vec<String> = Vec::new();
    let mut rx_handles: Vec<(
        String,
        tokio::sync::broadcast::Receiver<TransactionStatusEvent>,
    )> = Vec::new();
    let mut last_activity = tokio::time::Instant::now();
    let mut ping_ticker = tokio::time::interval(PING_INTERVAL);
    ping_ticker.tick().await; // consume the immediate first tick
    // One receiver for the whole connection. Events for all subscriptions
    // arrive on the same broadcast channel and are filtered by tx_id below.
    let mut rx = state.tx_status_tx.subscribe();
    let mut subscriptions: HashSet<String> = HashSet::new();

    loop {
        tokio::select! {
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_activity = tokio::time::Instant::now();
                        match serde_json::from_str::<SubscribeMessage>(&text) {
                            Ok(sub_msg) => {
                                if sub_msg.action == "subscribe" {
                                    if subscriptions.len() >= MAX_SUBSCRIPTIONS_PER_CONNECTION {
                                        warn!(
                                            "Client hit subscription limit of {}",
                                            MAX_SUBSCRIPTIONS_PER_CONNECTION
                                        );
                                        let response = json!({
                                            "msg_type": "error",
                                            "data": {"message": "subscription limit reached"},
                                        });
                                        if let Err(e) = sender.send(Message::Text(response.to_string())).await {
                                            error!("Failed to send error: {}", e);
                                            break;
                                        }
                                    } else {
                                        info!("Client subscribed to tx_id: {}", sub_msg.tx_id);
                                        subscriptions.push(sub_msg.tx_id.clone());
                                        state.add_subscriber(sub_msg.tx_id.clone());
                                        let rx = state.tx_status_tx.subscribe();
                                        rx_handles.push((sub_msg.tx_id.clone(), rx));
                                    info!("Client subscribed to tx_id: {}", sub_msg.tx_id);
                                    subscriptions.insert(sub_msg.tx_id.clone());
                                    state.add_subscriber(sub_msg.tx_id.clone());

                                        let response = json!({
                                            "msg_type": "subscribed",
                                            "data": {
                                                "tx_id": sub_msg.tx_id,
                                                "status": "subscribed",
                                            },
                                        });

                                        if let Err(e) = sender.send(Message::Text(response.to_string())).await {
                                            error!("Failed to send subscription confirmation: {}", e);
                                            break;
                                        }
                                    }
                                } else if sub_msg.action == "unsubscribe" {
                                    info!("Client unsubscribed from tx_id: {}", sub_msg.tx_id);
                                    subscriptions.remove(&sub_msg.tx_id);
                                    state.remove_subscriber(&sub_msg.tx_id);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse WebSocket message: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_activity = tokio::time::Instant::now();
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("WebSocket client disconnected");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            result = recv_matching(&mut rx, &subscriptions) => {
                match result {
                    Some(event) => {
                        let response = json!({
                            "msg_type": "status_update",
                            "data": {
                                "tx_id": event.tx_id,
                                "status": event.status,
                                "timestamp": event.timestamp,
                                "message": event.message,
                            },
                        });

                        if let Err(e) = sender.send(Message::Text(response.to_string())).await {
                            error!("Failed to send status update: {}", e);
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = ping_ticker.tick() => {
                if last_activity.elapsed() >= IDLE_TIMEOUT {
                    info!("WebSocket client timed out due to inactivity");
                    break;
                }
                if let Err(e) = sender.send(Message::Ping(vec![])).await {
                    error!("Failed to send ping: {}", e);
                    break;
                }
            }
        }
    }

    // Deferred cleanup: remove all subscriptions regardless of how the loop exited
    // (normal Close/None, WebSocket error, or send failure).
    for tx_id in &subscriptions {
        state.remove_subscriber(tx_id);
    }
    state.release_ws_connection();

    info!("WebSocket handler exiting");
}

/// Wait for the next broadcast event that matches one of the subscribed tx_ids.
/// Returns `None` if the sender is dropped (server shutting down).
async fn recv_matching(
    rx: &mut tokio::sync::broadcast::Receiver<TransactionStatusEvent>,
    subscriptions: &HashSet<String>,
) -> Option<TransactionStatusEvent> {
    if subscriptions.is_empty() {
        // Nothing subscribed — park indefinitely until there are subscriptions.
        std::future::pending::<Option<TransactionStatusEvent>>().await
    } else {
        loop {
            match rx.recv().await {
                Ok(event) if subscriptions.contains(&event.tx_id) => return Some(event),
                Ok(_) => continue, // event for a different tx_id; keep waiting
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("WebSocket receiver lagged, skipped {} events", n);
                    continue;
                }
                Err(_) => return None, // sender dropped
            }
        }
    }
}
