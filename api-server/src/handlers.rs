use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tracing::{error, info};
use utoipa::ToSchema;

use crate::{
    state::AppState,
    types::{
        ErrorCode, ErrorResponse, FeeEstimate, HealthResponse, RouteListResponse, RouteEntryResponse,
        SimulateRequest, SimulateResponse, SimulationDetail,
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
    if req.target.is_empty() || req.function.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::with_field(
                ErrorCode::ValidationError,
                "target and function are required",
                "target",
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

    let breakdown = state
        .rpc
        .simulate(&req.target, &req.function, req.amount, req.network_load_bps)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::RpcError, e.to_string())),
            )
        })?;

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
        },
        message: if breakdown.would_succeed {
            "Simulation successful".to_string()
        } else {
            "Simulation indicates transaction would fail".to_string()
        },
    }))
}

#[utoipa::path(
    get,
    path = "/routes/{name}",
    params(("name" = String, Path, description = "Route name")),
    responses(
        (status = 200, description = "Route entry returned", body = RouteEntryResponse),
        (status = 404, description = "Route not found", body = ErrorResponse),
        (status = 500, description = "RPC error", body = ErrorResponse),
    )
)]
/// GET /routes/:name
///
/// Calls router-core::get_route(name) via the Soroban RPC and returns the
/// full RouteEntry as JSON. Returns 404 if the route does not exist.
pub async fn get_route(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    info!(route = %name, "fetching route");

    match state.rpc.get_route(&name).await {
        Ok(Some(entry)) => Ok((StatusCode::OK, Json(entry))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                ErrorCode::NotFound,
                format!("route '{}' not found", name),
            )),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(ErrorCode::RpcError, e.to_string())),
        )),
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
