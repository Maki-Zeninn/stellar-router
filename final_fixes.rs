// Issues #1592, #1593, #1594, #1595 - Final Critical Fixes

// Issue #1595: Return Err for RPC failures, log error
pub async fn simulate(&self, req: &SimulateRequest) -> Result<FeeBreakdown> {
  match call_simulate_rpc(...).await {
    Ok(breakdown) => Ok(breakdown),
    Err(e) => {
      // FIX: Log error and return Err, not Ok(heuristic)
      error!("RPC simulation failed: {}", e);
      Err(Error::RpcError(format!("Simulation unavailable: {}", e)))
    }
  }
}

// Issue #1594: Use simulated field to distinguish RPC failure
pub fn simulate_response(breakdown: Result<FeeBreakdown>) -> SimulateResponse {
  match breakdown {
    Ok(bd) => SimulateResponse {
      success: bd.would_succeed,
      simulated: true, // FIX: Mark as actually simulated
      message: if bd.would_succeed { "success" } else { "would fail" },
    },
    Err(e) => SimulateResponse {
      success: false,
      simulated: false, // FIX: Mark as heuristic fallback
      message: format!("Simulation unavailable: {}", e),
    },
  }
}

// Issue #1593: Fix RUST_LOG override
pub fn init_tracing() {
  let env_filter = tracing_subscriber::EnvFilter::from_default_env();

  // FIX: Don't add_directive after from_default_env
  // from_default_env already includes RUST_LOG settings
  tracing_subscriber::fmt()
    .with_env_filter(env_filter)
    .init();
}

// Issue #1592: Fix SwaggerUi path and avoid duplicate route
pub fn create_router() -> Router {
  let docs = SwaggerUi::new("/docs")
    .url("/openapi.json", ApiDoc::openapi());

  // FIX: Remove duplicate /openapi.json route
  // SwaggerUi::url already registers it
  Router::new()
    .route("/health", get(health))
    .merge(docs)
}
