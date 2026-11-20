use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info};

use crate::{
    state::AppState,
    types::{
        ErrorCode, ErrorResponse, FeeEstimate, HealthResponse, RouteEntryResponse,
        RouteListResponse, SimulateRequest, SimulateResponse, SimulationDetail,
        TransactionStatus, TransactionStatusEvent,
    },
};

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse),
        (status = 503, description = "RPC is unavailable", body = HealthResponse),
    )
)]
/// GET /health
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    match state.rpc.health_check().await {
        Ok(()) => (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ok".to_string(),
                rpc: "up".to_string(),
            }),
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "degraded".to_string(),
                rpc: "down".to_string(),
            }),
        ),
    }
}

#[utoipa::path(
    post,
    path = "/simulate",
    request_body = SimulateRequest,
    responses(
        (status = 200, description = "Simulation completed", body = SimulateResponse),
        (status = 400, description = "Validation failed", body = ErrorResponse),
        (status = 500, description = "RPC or simulation error", body = ErrorResponse),
    )
)]
/// POST /simulate
///
/// Calls the Soroban RPC `simulateTransaction` endpoint to get real fee
/// estimates. Falls back to heuristic estimates if the RPC is unavailable.
pub async fn simulate(
    State(state): State<AppState>,
    Json(req): Json<SimulateRequest>,
) -> Result<Json<SimulateResponse>, (StatusCode, Json<ErrorResponse>)> {
    if req.target.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::with_field(
                ErrorCode::ValidationError,
                "target and function are required",
                "target",
            )),
        ));
    }

    if req.function.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::with_field(
                ErrorCode::ValidationError,
                "target and function are required",
                "function",
            )),
        ));
    }

    if req.target.len() != 56 || !req.target.starts_with('C') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::with_field(
                ErrorCode::ValidationError,
                "target must be a 56-character Stellar contract ID starting with C",
                "target",
            )),
        ));
    }

    info!(target = %req.target, function = %req.function, "simulating transaction");

    let tx_id = req.tx_id.clone().filter(|id| !id.is_empty());
    if let Some(id) = &tx_id {
        state.broadcast_status(status_event(id, TransactionStatus::Pending, "simulation started"));
    }

    let breakdown = match state
        .rpc
        .simulate(&req.target, &req.function, req.amount, req.network_load_bps)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            if let Some(id) = &tx_id {
                state.broadcast_status(status_event(
                    id,
                    TransactionStatus::Failed,
                    &format!("simulation error: {e}"),
                ));
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::RpcError, e.to_string())),
            ));
        }
    };

    if let Some(id) = &tx_id {
        let (status, message) = if breakdown.would_succeed {
            (TransactionStatus::Confirmed, "simulation successful")
        } else {
            (TransactionStatus::Failed, "simulation indicates transaction would fail")
        };
        state.broadcast_status(status_event(id, status, message));
    }

    Ok(Json(SimulateResponse {
        success: breakdown.would_succeed,
        estimated_fees: FeeEstimate {
            base_fee: breakdown.base_fee,
            resource_fee: breakdown.resource_fee,
            total_fee: breakdown.total_fee,
            surge_multiplier: breakdown.surge_multiplier,
            high_load: breakdown.high_load,
            fee_estimated: breakdown.fee_estimated,
        },
        simulation: SimulationDetail {
            target: req.target,
            function: req.function,
            would_succeed: breakdown.would_succeed,
            diagnostics: breakdown.diagnostics,
        },
        message: if breakdown.would_succeed {
            "Simulation successful".to_string()
        } else {
            "Simulation indicates transaction would fail".to_string()
        },
    }))
}

/// Builds a `TransactionStatusEvent` stamped with the current Unix time (seconds).
fn status_event(tx_id: &str, status: TransactionStatus, message: &str) -> TransactionStatusEvent {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string();
    TransactionStatusEvent {
        tx_id: tx_id.to_string(),
        status,
        timestamp,
        message: Some(message.to_string()),
    }
}

#[utoipa::path(
    get,
    path = "/routes/{name}",
    params(("name" = String, Path, description = "Route name")),
    responses(
        (status = 200, description = "Route entry returned", body = RouteEntryResponse),
        (status = 400, description = "Route name validation failed", body = ErrorResponse),
        (status = 404, description = "Route not found", body = ErrorResponse),
        (status = 503, description = "Router core contract not configured", body = ErrorResponse),
        (status = 500, description = "RPC or contract simulation error", body = ErrorResponse),
    )
)]
/// GET /routes/:name
///
/// Calls router-core::get_route(name) via the Soroban RPC and returns the
/// full RouteEntry as JSON. Returns 404 only if the route does not exist;
/// RPC or contract simulation errors return 500.
pub async fn get_route(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    if let Err(message) = validate_route_name(&name) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::with_field(
                ErrorCode::ValidationError,
                message,
                "name",
            )),
        ));
    }
    info!(route = %name, "fetching route");

    // Same guard as `list_routes`: fail fast with a clear 503 instead of
    // sending a pointless RPC request with an empty contract ID and mapping
    // the resulting failure to a misleading 500.
    if state.router_core_contract_id.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new(
                ErrorCode::InternalError,
                "ROUTER_CORE_CONTRACT_ID not configured",
            )),
        ));
    }

    match state.rpc.get_route(&name).await {
        Ok(Some(entry)) => Ok((StatusCode::OK, Json(entry))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                ErrorCode::NotFound,
                format!("route '{name}' not found"),
            )),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(ErrorCode::RpcError, e.to_string())),
        )),
    }
}

/// Keep API route lookup validation aligned with router-core's route-name
/// policy. The contract accepts 1–64 ASCII alphanumeric, '-' or '/' bytes.
/// A slash must be percent-encoded by HTTP clients when it appears inside the
/// path parameter so the router still receives it as one name segment.
fn validate_route_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("route name must not be empty".to_string());
    }
    if name.len() > 64 {
        return Err("route name must be at most 64 characters".to_string());
    }
    if name
        .bytes()
        .all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'/'))
    {
        Ok(())
    } else {
        Err("route name may contain only letters, numbers, '-' and '/'".to_string())
    }
}

#[cfg(test)]
mod route_name_tests {
    use super::validate_route_name;

    #[test]
    fn rejects_empty_route_name() {
        assert!(validate_route_name("").is_err());
    }

    #[test]
    fn rejects_route_name_longer_than_64_bytes() {
        assert!(validate_route_name(&"a".repeat(65)).is_err());
    }

    #[test]
    fn rejects_route_name_with_bad_characters() {
        assert!(validate_route_name("oracle?admin").is_err());
    }

    #[test]
    fn accepts_router_core_route_name_characters() {
        assert!(validate_route_name("dex/v1-quote").is_ok());
    }
}

#[utoipa::path(
    get,
    path = "/routes",
    responses(
        (status = 200, description = "List of routes", body = RouteListResponse),
        (status = 503, description = "Router core contract not configured", body = ErrorResponse),
        (status = 500, description = "RPC error", body = ErrorResponse),
    )
)]
/// GET /routes
///
/// Calls `get_all_routes` on the router-core contract via Soroban RPC and
/// returns the list of registered route names as JSON.
pub async fn list_routes(
    State(state): State<AppState>,
) -> Result<Json<RouteListResponse>, (StatusCode, Json<ErrorResponse>)> {
    if state.router_core_contract_id.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse::new(
                ErrorCode::InternalError,
                "ROUTER_CORE_CONTRACT_ID not configured",
            )),
        ));
    }

    let routes = state
        .rpc
        .get_all_routes(&state.router_core_contract_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch routes: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::RpcError, e.to_string())),
            )
        })?;

    info!("Returning {} routes", routes.len());
    Ok(Json(RouteListResponse { routes }))
}
