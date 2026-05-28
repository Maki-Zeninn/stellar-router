use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures_util::stream::{FuturesUnordered, StreamExt};
use serde_json::json;
use tracing::{error, info, warn};

use crate::{
    state::AppState,
    types::{SubscribeMessage, TransactionStatusEvent},
};

/// WebSocket upgrade handler
pub async fn ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    info!("WebSocket client connected");

    let mut subscriptions = Vec::new();
    let mut rx_handles = Vec::new();

    loop {
        tokio::select! {
            // Handle incoming messages from client
            msg = receiver.recv() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Text(text))) => {
                        match serde_json::from_str::<SubscribeMessage>(&text) {
                            Ok(sub_msg) => {
                                if sub_msg.action == "subscribe" {
                                    info!("Client subscribed to tx_id: {}", sub_msg.tx_id);
                                    subscriptions.push(sub_msg.tx_id.clone());
                                    state.add_subscriber(sub_msg.tx_id.clone());

                                    // Create broadcast receiver for this subscription
                                    let mut rx = state.tx_status_tx.subscribe();
                                    rx_handles.push((sub_msg.tx_id.clone(), rx));

                                    // Send confirmation
                                    let response = json!({
                                        "msg_type": "subscribed",
                                        "data": {
                                            "tx_id": sub_msg.tx_id,
                                            "status": "subscribed"
                                        }
                                    });

                                    if let Err(e) = sender.send(axum::extract::ws::Message::Text(
                                        response.to_string(),
                                    )).await {
                                        error!("Failed to send subscription confirmation: {}", e);
                                        break;
                                    }
                                } else if sub_msg.action == "unsubscribe" {
                                    info!("Client unsubscribed from tx_id: {}", sub_msg.tx_id);
                                    subscriptions.retain(|id| id != &sub_msg.tx_id);
                                    state.remove_subscriber(&sub_msg.tx_id);
                                    rx_handles.retain(|(id, _)| id != &sub_msg.tx_id);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse WebSocket message: {}", e);
                            }
                        }
                    }
                    Some(Ok(axum::extract::ws::Message::Close(_))) => {
                        info!("WebSocket client disconnected");
                        for tx_id in subscriptions {
                            state.remove_subscriber(&tx_id);
                        }
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        info!("WebSocket connection closed");
                        for tx_id in subscriptions {
                            state.remove_subscriber(&tx_id);
                        }
                        break;
                    }
                    _ => {}
                }
            }

            // Handle broadcast messages — poll all receivers concurrently
            // without busy-looping by using FuturesUnordered.
            result = async {
                if rx_handles.is_empty() {
                    std::future::pending::<Option<(String, TransactionStatusEvent)>>().await
                } else {
                    let mut futs: FuturesUnordered<_> = rx_handles
                        .iter_mut()
                        .map(|(tx_id, rx)| {
                            let id = tx_id.clone();
                            async move {
                                match rx.recv().await {
                                    Ok(event) => Some((id, event)),
                                    Err(_) => None,
                                }
                            }
                        })
                        .collect();
                    loop {
                        match futs.next().await {
                            Some(Some(pair)) => return Some(pair),
                            Some(None) => continue,
                            None => return None,
                        }
                    }
                }
            } => {
                if let Some((tx_id, event)) = result {
                    let response = json!({
                        "msg_type": "status_update",
                        "data": {
                            "tx_id": event.tx_id,
                            "status": event.status,
                            "timestamp": event.timestamp,
                            "message": event.message
                        }
                    });

                    if let Err(e) = sender.send(axum::extract::ws::Message::Text(
                        response.to_string(),
                    )).await {
                        error!("Failed to send status update: {}", e);
                        break;
                    }
                }
            }
        }
    }

    info!("WebSocket handler exiting");
}
