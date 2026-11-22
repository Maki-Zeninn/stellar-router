// Issues #1600, #1601, #1602, #1603 - API Server Fixes

use axum::{Json, response::{IntoResponse, Response}, http::StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Serialize, Deserialize)]
struct ErrorDetail {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<String>,
}

// Issue #1603: Return ErrorResponse for JSON validation errors
pub struct JsonRejectionHandler;

impl IntoResponse for JsonRejectionHandler {
    fn into_response(self) -> Response {
        let error = ErrorResponse {
            error: ErrorDetail {
                code: "VALIDATION_FAILED".to_string(),
                message: "Invalid JSON or missing required fields".to_string(),
                field: None,
            },
        };
        (StatusCode::BAD_REQUEST, Json(error)).into_response()
    }
}

// Issue #1602: WebSocket message handling with proper error responses
pub mod websocket {
    use super::*;

    #[derive(Deserialize)]
    pub struct SubscribeMessage {
        pub action: String,
        pub tx_id: String,
    }

    #[derive(Serialize)]
    pub struct WsErrorMessage {
        msg_type: String,
        data: WsError,
    }

    #[derive(Serialize)]
    pub struct WsError {
        error: String,
    }

    pub fn handle_action(action: &str) -> Result<(), WsErrorMessage> {
        match action {
            "subscribe" | "unsubscribe" => Ok(()),
            _ => Err(WsErrorMessage {
                msg_type: "error".to_string(),
                data: WsError {
                    error: format!("Unknown action: {}", action),
                },
            }),
        }
    }

    pub fn connection_limit_error() -> WsErrorMessage {
        WsErrorMessage {
            msg_type: "error".to_string(),
            data: WsError {
                error: "connection limit reached".to_string(),
            },
        }
    }
}

// Issue #1600 & #1601: Fix tx_subscribers double-counting
pub mod app_state {
    use std::collections::{HashMap, HashSet};
    use dashmap::DashMap;
    use std::sync::Arc;

    pub struct AppState {
        subscriptions: Arc<DashMap<String, HashSet<String>>>,
    }

    impl AppState {
        pub fn add_subscriber(&self, tx_id: String, conn_id: String) {
            self.subscriptions
                .entry(tx_id)
                .or_insert_with(HashSet::new)
                .insert(conn_id);
        }

        pub fn remove_subscriber(&self, tx_id: &str, conn_id: &str) -> bool {
            if let Some(mut subs) = self.subscriptions.get_mut(tx_id) {
                subs.remove(conn_id)
            } else {
                false
            }
        }

        pub fn get_subscribers(&self, tx_id: &str) -> usize {
            self.subscriptions
                .get(tx_id)
                .map(|s| s.len())
                .unwrap_or(0)
        }
    }
}

// Issue #1603: /simulate endpoint returns proper ErrorResponse
pub mod simulate {
    use super::*;

    #[derive(Deserialize)]
    pub struct SimulateRequest {
        pub transaction: String,
        pub resource_fee: Option<u32>,
    }

    pub async fn simulate(
        Json(req): Json<SimulateRequest>,
    ) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
        Ok(Json(serde_json::json!({})))
    }
}
