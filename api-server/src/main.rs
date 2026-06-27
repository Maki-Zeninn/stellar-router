mod handlers;
mod rpc;
mod state;
mod types;
mod websocket;

#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderValue, Method, Request},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use clap::Parser;
use std::net::SocketAddr;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{info, warn};
use tracing::{info, info_span, warn, Instrument};

use crate::state::AppState;

#[derive(Parser, Debug)]
#[command(name = "router-api-server")]
#[command(
    about = "API server for stellar-router with transaction simulation and WebSocket tracking"
)]
struct Args {
    /// Listen address (default: 127.0.0.1:8080)
    #[arg(long, env = "LISTEN_ADDR", default_value = "127.0.0.1:8080")]
    listen: String,

    /// Soroban RPC endpoint URL
    #[arg(long, env = "SOROBAN_RPC_URL")]
    rpc_url: String,

    /// Router execution contract ID
    #[arg(long, env = "ROUTER_EXECUTION_CONTRACT_ID")]
    execution_contract_id: String,

    /// Router core contract ID (for GET /routes)
    #[arg(long, env = "ROUTER_CORE_CONTRACT_ID", default_value = "")]
    router_core_contract_id: String,

    /// Allowed CORS origins, comma-separated. Use "*" to allow any origin (dev only).
    /// Omit to disable cross-origin requests (production default).
    #[arg(long, env = "CORS_ORIGINS", value_delimiter = ',')]
    cors_origins: Vec<String>,
    /// Seconds to wait for in-flight requests to complete on shutdown (default: 30)
    #[arg(long, env = "SHUTDOWN_TIMEOUT_SECS", default_value = "30")]
    shutdown_timeout_secs: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    info!("Starting router-api-server");
    info!("Listen address: {}", args.listen);
    info!("RPC URL: {}", args.rpc_url);

    let state = AppState::new(
        args.rpc_url,
        args.execution_contract_id,
        args.router_core_contract_id,
    );

    let cors = build_cors_layer(&args.cors_origins);

    let app = Router::new()
        .route("/health", get(handlers::health))
        .route("/simulate", post(handlers::simulate))
        .route("/routes", get(handlers::list_routes))
        .route("/routes/:name", get(handlers::get_route))
        .route("/ws", get(websocket::ws_handler))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(cors)
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(state);

    let addr: SocketAddr = args
        .listen
        .parse()
        .with_context(|| format!("invalid listen address: {}", args.listen))?;

    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;

    let drain_timeout = std::time::Duration::from_secs(args.shutdown_timeout_secs);
    let serve = axum::serve(listener, app).with_graceful_shutdown(shutdown_signal());

    match tokio::time::timeout(drain_timeout, serve).await {
        Ok(result) => result?,
        Err(_) => {
            warn!(
                "Graceful shutdown drain timed out after {}s, forcing exit",
                args.shutdown_timeout_secs
            );
        }
    }

    Ok(())
}

/// Attach a unique `request_id` (UUID v4) to every request span and response header.
///
/// All logs emitted while handling the request inherit the `request_id` field
/// from the enclosing span, enabling correlation across log lines for a single
/// request. The ID is also returned to the client in the `X-Request-Id` header.
async fn request_id_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();
    let method = req.method().clone();
    let uri = req.uri().clone();

    let span = info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        uri = %uri,
    );

    async move {
        info!(request_id = %request_id, %method, %uri, "incoming request");
        let mut response = next.run(req).await;
        let status = response.status().as_u16();
        info!(request_id = %request_id, status, "request complete");

        if let Ok(val) = HeaderValue::from_str(&request_id) {
            response.headers_mut().insert("x-request-id", val);
        }
        response
    }
    .instrument(span)
    .await
}

fn build_cors_layer(origins: &[String]) -> CorsLayer {
    let allow_methods = [Method::GET, Method::POST, Method::OPTIONS];
    let allow_headers = [header::CONTENT_TYPE, header::AUTHORIZATION];

    if origins.is_empty() {
        return CorsLayer::new();
    }

    let allow_origin = if origins.iter().any(|o| o == "*") {
        AllowOrigin::any()
    } else {
        let parsed: Vec<_> = origins.iter().filter_map(|o| o.parse().ok()).collect();
        AllowOrigin::list(parsed)
    };

    CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods(allow_methods)
        .allow_headers(allow_headers)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, draining in-flight requests...");
}
