use axum::{body::Body, http::{Request, StatusCode}, routing::{get, post}, Router};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{
    handlers,
    state::AppState,
    types::{
        RouteDetails,
        SimulateRequest,
        SimulateResponse,
        TransactionStatus,
        TransactionStatusEvent,
    },
};

/// Valid 56-char Stellar contract ID for use in tests.
const VALID_CONTRACT_ID: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";

fn test_app() -> Router {
    let state = AppState::new(
        "http://localhost:1".to_string(),
        "".to_string(),
        "".to_string(),
    );

    Router::new()
        .route("/health", get(handlers::health))
        .route("/simulate", post(handlers::simulate))
        .route("/routes/:name", get(handlers::get_route))
        .with_state(state)
}

#[tokio::test]
async fn test_health_returns_200() {
    let app = test_app();
    let resp = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_health_returns_ok_body() {
    let app = test_app();
    let resp = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
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
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let parsed: SimulateResponse = serde_json::from_slice(&bytes).unwrap();
    assert!(parsed.estimated_fees.base_fee > 0);
    assert!(parsed.estimated_fees.total_fee >= parsed.estimated_fees.base_fee);
    assert_eq!(parsed.simulation.target, VALID_CONTRACT_ID);
    assert_eq!(parsed.simulation.function, "transfer");
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
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
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
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"].as_str().unwrap().contains("56-character"));
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
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"].is_string());
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
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("error").is_some());
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
