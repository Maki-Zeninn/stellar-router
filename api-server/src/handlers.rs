use axum::{extract::{Path, State}, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use tracing::{error, info};

use crate::{
    state::AppState,
    types::{
        ErrorResponse,
        FeeEstimate,
        RouteBreakdown,
        RouteDetails,
        SimulateRequest,
        SimulateResponse,
        SimulationDetail,
    },
};

/// GET /health
pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

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
            Json(ErrorResponse { error: "target and function are required".to_string() }),
        ));
    }

    if req.target.len() != 56 || !req.target.starts_with('C') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "target must be a 56-character Stellar contract ID starting with C".to_string(),
            }),
        ));
    }

    info!(target = %req.target, function = %req.function, "simulating transaction");

    let breakdown = state
        .rpc
        .simulate(&req.target, &req.function, req.amount, req.network_load_bps)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: e.to_string() })))?;

    Ok(Json(SimulateResponse {
        success: breakdown.would_succeed,
        estimated_fees: FeeEstimate {
            base_fee: breakdown.base_fee,
            resource_fee: breakdown.resource_fee,
            total_fee: breakdown.total_fee,
            surge_multiplier: breakdown.surge_multiplier,
            high_load: breakdown.high_load,
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
            Json(ErrorResponse { error: format!("route '{}' not found", name) }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e.to_string() }),
        )),
    }
}

/// GET /routes
///
/// Calls `get_all_routes` on the router-core contract via Soroban RPC and
/// returns the list of registered route names as JSON.
pub async fn list_routes(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if state.router_core_contract_id.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "ROUTER_CORE_CONTRACT_ID not configured".to_string(),
        ));
    }

    let routes = state
        .rpc
        .get_all_routes(&state.router_core_contract_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch routes: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    info!("Returning {} routes", routes.len());
    Ok(Json(json!({ "routes": routes })))
}
