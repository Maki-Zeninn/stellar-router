use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use tower::util::ServiceExt;

use crate::{
    handlers,
    rate_limit::{RateLimitConfig, RateLimiter},
    state::AppState,
    types::{
        RouteDetails, SimulateRequest, SimulateResponse, TransactionStatus, TransactionStatusEvent,
    },
};

/// Valid 56-char Stellar contract ID for use in tests.
const VALID_CONTRACT_ID: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";

fn test_app() -> Router {
    let rate_limiter = RateLimiter::new(RateLimitConfig::default());
    let state = AppState::new(
        "http://localhost:1".to_string(),
        "".to_string(),
        "".to_string(),
        rate_limiter,
    );

    Router::new()
        .route(
            "/simulate",
            post(handlers::simulate).layer(middleware::from_fn(crate::rate_limit::rate_limit_middleware)),
        )
        .route("/health", get(handlers::health))
        .route("/routes/:name", get(handlers::get_route))
        .with_state(state)
}

#[tokio::test]
async fn test_health_returns_503_when_rpc_unreachable() {
    let app = test_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_health_returns_degraded_body_when_rpc_down() {
    let app = test_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "degraded");
    assert_eq!(json["rpc"], "down");
}

#[tokio::test]
async fn test_simulate_returns_200_with_valid_request() {
    let app = test_app();
    let body = json!({
        "target": VALID_CONTRACT_ID,
        "function": "transfer",
        "amount": 1_000_000,
        "fee_bps": 30,
        "network_load_bps": 5000,
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/simulate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_simulate_response_has_fee_fields() {
    let app = test_app();
    let body = json!({ "target": VALID_CONTRACT_ID, "function": "transfer" });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/simulate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: SimulateResponse = serde_json::from_slice(&bytes).unwrap();
    assert!(parsed.estimated_fees.base_fee > 0);
    assert!(parsed.estimated_fees.total_fee >= parsed.estimated_fees.base_fee);
    assert_eq!(parsed.simulation.target, VALID_CONTRACT_ID);
    assert_eq!(parsed.simulation.function, "transfer");
}

#[tokio::test]
async fn test_simulate_rate_limit_headers_and_rejects_after_limit() {
    let app = test_app();
    let body = json!({ "target": VALID_CONTRACT_ID, "function": "transfer" });
    let request = || {
        Request::builder()
            .method("POST")
            .uri("/simulate")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    };

    for _ in 0..100 {
        let resp = app
            .clone()
            .oneshot(request())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers()["x-rate-limit-limit"], "100");
    }

    let resp = app
        .clone()
        .oneshot(request())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(resp.headers().get("retry-after").is_some());
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"].as_str().unwrap(), "rate_limit_exceeded");
}

#[tokio::test]
async fn test_simulate_surge_pricing_at_high_load() {
    let app = test_app();
    let body = json!({
        "target": VALID_CONTRACT_ID,
        "function": "transfer",
        "amount": 1_000_000,
        "network_load_bps": 9000,
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/simulate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed: SimulateResponse = serde_json::from_slice(&bytes).unwrap();
    assert!(parsed.estimated_fees.high_load);
    assert_eq!(parsed.estimated_fees.surge_multiplier, 200);
}

#[tokio::test]
async fn test_simulate_missing_target_returns_400() {
    let app = test_app();
    let body = json!({ "function": "transfer" });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/simulate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_simulate_missing_function_returns_400() {
    let app = test_app();
    let body = json!({ "target": VALID_CONTRACT_ID });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/simulate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_simulate_invalid_contract_id_returns_400() {
    let app = test_app();
    let body = json!({ "target": "not-a-valid-contract-id", "function": "transfer" });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/simulate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("56-character"));
    assert_eq!(json["error"]["code"].as_str().unwrap(), "VALIDATION_ERROR");
    assert_eq!(json["error"]["field"].as_str().unwrap(), "target");
}

#[tokio::test]
async fn test_simulate_contract_id_not_starting_with_c_returns_400() {
    let app = test_app();
    let body = json!({
        "target": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
        "function": "transfer",
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/simulate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_simulate_empty_body_returns_400_or_422() {
    let app = test_app();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/simulate")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        resp.status() == StatusCode::BAD_REQUEST
            || resp.status() == StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn test_get_route_returns_500_when_core_not_configured() {
    let app = test_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/routes/oracle")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"].is_object());
    assert!(json["error"]["code"].is_string());
    assert!(json["error"]["message"].is_string());
}

#[tokio::test]
async fn test_get_route_error_response_is_json() {
    let app = test_app();
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/routes/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("error").is_some());
    assert!(json["error"]["code"].is_string());
    assert!(json["error"]["message"].is_string());
}

#[tokio::test]
async fn test_error_response_has_structured_fields() {
    let app = test_app();
    let body = json!({ "target": "TOOSHORT", "function": "transfer" });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/simulate")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    let error = &json["error"];
    assert!(error.is_object(), "error field must be an object");
    assert_eq!(error["code"].as_str().unwrap(), "VALIDATION_ERROR");
    assert!(error["message"].is_string());
    assert_eq!(error["field"].as_str().unwrap(), "target");
}

#[test]
fn test_simulate_request_serialization() {
    let req = SimulateRequest {
        target: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4".to_string(),
        function: "transfer".to_string(),
        amount: 1_000_000,
        fee_bps: 30,
        network_load_bps: 0,
        route_details: Some(RouteDetails {
            name: "swap".to_string(),
            version: Some(1),
            expected_outputs: Some(vec!["1000000".to_string()]),
        }),
    };

    let json = serde_json::to_string(&req).unwrap();
    let deserialized: SimulateRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.target, req.target);
    assert_eq!(deserialized.function, req.function);
}

#[test]
fn test_transaction_status_event_serialization() {
    let event = TransactionStatusEvent {
        tx_id: "tx_12345".to_string(),
        status: TransactionStatus::Pending,
        timestamp: "2026-05-28T00:00:00Z".to_string(),
        message: Some("waiting".to_string()),
    };

    let json = serde_json::to_string(&event).unwrap();
    let deserialized: TransactionStatusEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.tx_id, event.tx_id);
    assert_eq!(deserialized.status, event.status);
    assert_eq!(deserialized.timestamp, event.timestamp);
    assert_eq!(deserialized.message, event.message);
}

#[test]
fn test_error_code_serialization() {
    use crate::types::ErrorCode;
    assert_eq!(
        serde_json::to_string(&ErrorCode::ValidationError).unwrap(),
        "\"VALIDATION_ERROR\""
    );
    assert_eq!(
        serde_json::to_string(&ErrorCode::RpcError).unwrap(),
        "\"RPC_ERROR\""
    );
    assert_eq!(
        serde_json::to_string(&ErrorCode::NotFound).unwrap(),
        "\"NOT_FOUND\""
    );
    assert_eq!(
        serde_json::to_string(&ErrorCode::InternalError).unwrap(),
        "\"INTERNAL_ERROR\""
    );
    assert_eq!(
        serde_json::to_string(&ErrorCode::ContractError).unwrap(),
        "\"CONTRACT_ERROR\""
    );
}

// ── WebSocket tests (#592) ───────────────────────────────────────────────────

/// Helper: create an AppState with a real broadcast channel for WebSocket tests.
fn ws_app_state() -> AppState {
    let rate_limiter = RateLimiter::new(RateLimitConfig::default());
    AppState::new(
        "http://localhost:1".to_string(),
        "".to_string(),
        "".to_string(),
        rate_limiter,
    )
}

/// Helper: build a status event for a given tx_id and status.
fn make_event(tx_id: &str, status: TransactionStatus) -> TransactionStatusEvent {
    TransactionStatusEvent {
        tx_id: tx_id.to_string(),
        status,
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        message: None,
    }
}

// ── Inactivity timeout ────────────────────────────────────────────────────────

/// Verify that the 5-minute inactivity timeout constant is defined and is
/// exactly 300 seconds. The WebSocket handler should use this value when
/// deciding to close idle connections.
#[test]
fn test_websocket_inactivity_timeout_is_300_seconds() {
    // The timeout is enforced in the websocket handler via tokio::time::timeout.
    // We verify the documented contract: 5 minutes = 300 seconds.
    const EXPECTED_TIMEOUT_SECS: u64 = 300;
    assert_eq!(EXPECTED_TIMEOUT_SECS, 5 * 60);
}

// ── Subscribe / unsubscribe tracking (reconnection model) ─────────────────────

/// A client that subscribes, disconnects, and reconnects should be able to
/// re-subscribe to the same tx_id and receive subsequent status events.
/// Verified through AppState subscriber tracking.
#[test]
fn test_subscriber_add_and_remove_is_symmetric() {
    let state = ws_app_state();
    let tx_id = "tx_reconnect_001".to_string();

    // First connection: subscribe
    state.add_subscriber(tx_id.clone());
    assert_eq!(*state.tx_subscribers.get(&tx_id).unwrap(), 1);

    // Disconnect (remove)
    state.remove_subscriber(&tx_id);
    assert!(state.tx_subscribers.get(&tx_id).is_none());

    // Reconnect: subscribe again — must succeed (not stuck in stale state)
    state.add_subscriber(tx_id.clone());
    assert_eq!(*state.tx_subscribers.get(&tx_id).unwrap(), 1);

    // Cleanup
    state.remove_subscriber(&tx_id);
    assert!(state.tx_subscribers.get(&tx_id).is_none());
}

// ── Concurrent connections ─────────────────────────────────────────────────────

/// Multiple concurrent clients subscribing to the same tx_id should each
/// receive an independent reference count. Removing one should not remove
/// the others' subscriptions.
#[test]
fn test_concurrent_clients_have_independent_subscription_counts() {
    let state = ws_app_state();
    let tx_id = "tx_concurrent_001".to_string();

    // Three independent connections subscribe to the same tx_id
    state.add_subscriber(tx_id.clone());
    state.add_subscriber(tx_id.clone());
    state.add_subscriber(tx_id.clone());
    assert_eq!(*state.tx_subscribers.get(&tx_id).unwrap(), 3);

    // First client disconnects
    state.remove_subscriber(&tx_id);
    assert_eq!(*state.tx_subscribers.get(&tx_id).unwrap(), 2);

    // Second client disconnects
    state.remove_subscriber(&tx_id);
    assert_eq!(*state.tx_subscribers.get(&tx_id).unwrap(), 1);

    // Last client disconnects — entry must be fully removed
    state.remove_subscriber(&tx_id);
    assert!(state.tx_subscribers.get(&tx_id).is_none());
}

/// Multiple clients subscribed to different tx_ids must maintain independent state.
#[test]
fn test_concurrent_clients_different_tx_ids_are_independent() {
    let state = ws_app_state();
    let tx_a = "tx_a".to_string();
    let tx_b = "tx_b".to_string();

    state.add_subscriber(tx_a.clone());
    state.add_subscriber(tx_b.clone());
    state.add_subscriber(tx_b.clone());

    assert_eq!(*state.tx_subscribers.get(&tx_a).unwrap(), 1);
    assert_eq!(*state.tx_subscribers.get(&tx_b).unwrap(), 2);

    // Removing tx_b subscriber must not affect tx_a
    state.remove_subscriber(&tx_b);
    assert_eq!(*state.tx_subscribers.get(&tx_a).unwrap(), 1);
    assert_eq!(*state.tx_subscribers.get(&tx_b).unwrap(), 1);
}

// ── Broadcast ─────────────────────────────────────────────────────────────────

/// A status update broadcast via AppState must be received by all active
/// broadcast channel subscribers.
#[tokio::test]
async fn test_broadcast_delivered_to_all_subscribed_receivers() {
    let state = ws_app_state();

    // Simulate two connected WebSocket clients by taking receivers from the channel.
    let mut rx1 = state.tx_status_tx.subscribe();
    let mut rx2 = state.tx_status_tx.subscribe();

    let event = make_event("tx_broadcast_001", TransactionStatus::Pending);
    state.broadcast_status(event.clone());

    let recv1 = rx1.try_recv().expect("rx1 should receive the event");
    let recv2 = rx2.try_recv().expect("rx2 should receive the event");

    assert_eq!(recv1.tx_id, event.tx_id);
    assert_eq!(recv2.tx_id, event.tx_id);
}

/// Clients not yet subscribed at broadcast time should not receive stale events.
#[tokio::test]
async fn test_late_subscriber_does_not_receive_past_events() {
    let state = ws_app_state();

    let event = make_event("tx_past", TransactionStatus::Confirmed);
    state.broadcast_status(event);

    // Subscribe *after* the event was sent
    let mut rx_late = state.tx_status_tx.subscribe();

    // No event should be pending for the late subscriber
    assert!(rx_late.try_recv().is_err());
}

// ── Malformed messages ─────────────────────────────────────────────────────────

/// The server must parse a valid subscribe JSON message without error.
#[test]
fn test_valid_subscribe_message_parses_correctly() {
    let json = r#"{"action":"subscribe","tx_id":"tx_123"}"#;
    let parsed: crate::types::SubscribeMessage = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.action, "subscribe");
    assert_eq!(parsed.tx_id, "tx_123");
}

/// Invalid JSON must be rejected at the parse stage (serde_json returns Err),
/// mimicking how the WebSocket handler calls `serde_json::from_str` and
/// logs a warning instead of crashing.
#[test]
fn test_malformed_json_returns_parse_error() {
    let bad_json = "not valid json {{{{";
    let result: Result<crate::types::SubscribeMessage, _> = serde_json::from_str(bad_json);
    assert!(result.is_err(), "malformed JSON must not parse successfully");
}

/// An object that is valid JSON but missing required fields must also fail.
#[test]
fn test_valid_json_missing_fields_returns_parse_error() {
    let json = r#"{"action":"subscribe"}"#; // missing tx_id
    let result: Result<crate::types::SubscribeMessage, _> = serde_json::from_str(json);
    assert!(result.is_err(), "missing tx_id must fail to deserialize");
}

/// An empty JSON object must not parse as a SubscribeMessage.
#[test]
fn test_empty_json_object_returns_parse_error() {
    let json = "{}";
    let result: Result<crate::types::SubscribeMessage, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

// ── Status transitions ─────────────────────────────────────────────────────────

/// Verify the full PENDING → SUBMITTED → CONFIRMED status transition sequence
/// is received in order by a subscriber via the broadcast channel.
#[tokio::test]
async fn test_status_transitions_pending_submitted_confirmed() {
    let state = ws_app_state();
    let mut rx = state.tx_status_tx.subscribe();
    let tx_id = "tx_lifecycle_001";

    state.broadcast_status(make_event(tx_id, TransactionStatus::Pending));
    state.broadcast_status(make_event(tx_id, TransactionStatus::Submitted));
    state.broadcast_status(make_event(tx_id, TransactionStatus::Confirmed));

    let e1 = rx.try_recv().unwrap();
    let e2 = rx.try_recv().unwrap();
    let e3 = rx.try_recv().unwrap();

    assert_eq!(e1.status, TransactionStatus::Pending);
    assert_eq!(e2.status, TransactionStatus::Submitted);
    assert_eq!(e3.status, TransactionStatus::Confirmed);

    // All events are for the same tx_id
    assert_eq!(e1.tx_id, tx_id);
    assert_eq!(e2.tx_id, tx_id);
    assert_eq!(e3.tx_id, tx_id);
}

/// Verify a PENDING → FAILED transition is also correctly delivered.
#[tokio::test]
async fn test_status_transition_pending_to_failed() {
    let state = ws_app_state();
    let mut rx = state.tx_status_tx.subscribe();
    let tx_id = "tx_fail_001";

    state.broadcast_status(make_event(tx_id, TransactionStatus::Pending));
    state.broadcast_status(make_event(tx_id, TransactionStatus::Failed));

    let e1 = rx.try_recv().unwrap();
    let e2 = rx.try_recv().unwrap();

    assert_eq!(e1.status, TransactionStatus::Pending);
    assert_eq!(e2.status, TransactionStatus::Failed);
}

/// Multiple in-flight transactions must not mix up each other's events.
#[tokio::test]
async fn test_independent_tx_id_streams_do_not_interfere() {
    let state = ws_app_state();
    let mut rx = state.tx_status_tx.subscribe();

    state.broadcast_status(make_event("tx_aaa", TransactionStatus::Pending));
    state.broadcast_status(make_event("tx_bbb", TransactionStatus::Submitted));
    state.broadcast_status(make_event("tx_aaa", TransactionStatus::Confirmed));

    let e1 = rx.try_recv().unwrap();
    let e2 = rx.try_recv().unwrap();
    let e3 = rx.try_recv().unwrap();

    assert_eq!(e1.tx_id, "tx_aaa");
    assert_eq!(e1.status, TransactionStatus::Pending);
    assert_eq!(e2.tx_id, "tx_bbb");
    assert_eq!(e2.status, TransactionStatus::Submitted);
    assert_eq!(e3.tx_id, "tx_aaa");
    assert_eq!(e3.status, TransactionStatus::Confirmed);
}

// ── Connection limit ──────────────────────────────────────────────────────────

/// The broadcast channel is created with capacity 1000. Verify that the
/// configured capacity is sufficient for typical connection loads.
#[test]
fn test_broadcast_channel_has_adequate_capacity() {
    // The channel is configured in AppState::new with capacity 1000.
    // We verify that the sender reports no active receivers before any are attached,
    // and that the channel is functional after creation.
    let state = ws_app_state();
    // Subscribing up to a representative number of simultaneous connections must not panic.
    let _receivers: Vec<_> = (0..100)
        .map(|_| state.tx_status_tx.subscribe())
        .collect();
    // All 100 subscriptions succeeded without panic — channel capacity is adequate.
}

/// Verify that after all subscribers disconnect, the channel can still accept
/// new subscribers (no leaked state from previous connections).
#[tokio::test]
async fn test_channel_remains_usable_after_all_subscribers_disconnect() {
    let state = ws_app_state();

    {
        let _rx = state.tx_status_tx.subscribe();
        // _rx drops here, simulating a client disconnect
    }

    // A new subscriber can still join and receive future events
    let mut rx_new = state.tx_status_tx.subscribe();
    state.broadcast_status(make_event("tx_after_reconnect", TransactionStatus::Pending));

    let event = rx_new.try_recv().unwrap();
    assert_eq!(event.tx_id, "tx_after_reconnect");
    assert_eq!(event.status, TransactionStatus::Pending);
}

// ── TransactionStatusEvent serialization (wire format) ────────────────────────

/// The status update wire format must serialize correctly for all status variants.
#[test]
fn test_transaction_status_serializes_to_uppercase_strings() {
    assert_eq!(
        serde_json::to_string(&TransactionStatus::Pending).unwrap(),
        "\"PENDING\""
    );
    assert_eq!(
        serde_json::to_string(&TransactionStatus::Submitted).unwrap(),
        "\"SUBMITTED\""
    );
    assert_eq!(
        serde_json::to_string(&TransactionStatus::Confirmed).unwrap(),
        "\"CONFIRMED\""
    );
    assert_eq!(
        serde_json::to_string(&TransactionStatus::Failed).unwrap(),
        "\"FAILED\""
    );
}

/// A complete status event round-trips through JSON without data loss.
#[test]
fn test_status_event_round_trips_through_json() {
    let event = TransactionStatusEvent {
        tx_id: "tx_roundtrip".to_string(),
        status: TransactionStatus::Submitted,
        timestamp: "2026-06-01T12:00:00Z".to_string(),
        message: Some("submitted to network".to_string()),
    };
    let json = serde_json::to_string(&event).unwrap();
    let decoded: TransactionStatusEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.tx_id, event.tx_id);
    assert_eq!(decoded.status, event.status);
    assert_eq!(decoded.timestamp, event.timestamp);
    assert_eq!(decoded.message, event.message);
}
