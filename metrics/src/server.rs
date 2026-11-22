//! HTTP server that exposes the `/metrics`, `/health`, and `/ready` endpoints.
//!
//! Uses `axum` to serve:
//! - `GET /metrics` — Prometheus text format metrics
//! - `GET /health`  — simple liveness probe (returns `200 OK`)
//! - `GET /ready`   — readiness probe (returns `200 OK` after one completed
//!   scrape cycle while the RPC client is available)
//! - `GET /swagger-ui/` — Swagger UI for API documentation
//! - `GET /api-docs/openapi.json` — OpenAPI specification
//!
//! Every request is assigned a unique `request_id` that is logged and returned
//! in the `X-Request-Id` response header.

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{header, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use prometheus::proto::MetricType;
use prometheus::{Encoder, Registry, TextEncoder};
use std::net::SocketAddr;
use tracing::{info, info_span, Instrument};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::auth::AuthConfig;
use crate::logging::new_request_id;
use crate::openapi::ApiDoc;
use crate::rate_limit::{rate_limit_middleware, RateLimiter};
use crate::replay_protection::{NonceCache, ReplayProtectionConfig};

/// Shared server state.
#[derive(Clone)]
struct AppState {
    registry: Registry,
}

/// Start the HTTP server and block until it exits.
pub async fn serve(listen: String, registry: Registry, limiter: RateLimiter) -> Result<()> {
    let addr: SocketAddr = listen
        .parse()
        .with_context(|| format!("invalid listen address: {listen}"))?;

    let state = AppState { registry };
    let auth_config = AuthConfig::from_env();
    let replay_config = ReplayProtectionConfig::from_env();
    let nonce_cache = NonceCache::new(replay_config);

    // `/health` and `/ready` are liveness/readiness probes used by Docker's
    // HEALTHCHECK and orchestrators, which don't send an API key or a
    // `X-Nonce` header. They're registered *after* `route_layer`, so — unlike
    // `.layer()` — the auth and replay-protection middleware below only wrap
    // the routes already present on the router (`/metrics` and the Swagger
    // UI), not these two. Rate limiting and the request-id middleware still
    // apply to every route via `.layer()`.
    let app = Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .route("/metrics", get(metrics_handler))
        .route_layer(middleware::from_fn_with_state(
            nonce_cache,
            crate::replay_protection::replay_protection_middleware,
        ))
        .route_layer(middleware::from_fn_with_state(
            auth_config,
            crate::auth::auth_middleware,
        ))
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .layer(middleware::from_fn_with_state(
            limiter,
            rate_limit_middleware,
        ))
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(state);

    info!(%addr, "HTTP server listening");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind to {addr}"))?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("HTTP server error")?;

    Ok(())
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Attach a unique `request_id` to every request span and response header.
async fn request_id_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    let request_id = new_request_id();
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

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /metrics` — render all registered metrics in Prometheus text format.
///
/// Returns Prometheus-formatted metrics for all registered gauges, counters, and histograms.
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "metrics",
    responses(
        (status = 200, description = "Prometheus metrics in text format", content_type = "text/plain; version=0.0.4"),
        (status = 400, description = "Missing `X-Nonce` header (replay protection)"),
        (status = 401, description = "Missing or invalid API key"),
        (status = 409, description = "Duplicate `X-Nonce` (replayed request)"),
        (status = 429, description = "Rate limit exceeded",
            headers(("Retry-After" = u64, description = "Seconds until the rate-limit window resets"))),
        (status = 500, description = "Failed to encode metrics"),
    ),
    security(("api_key" = []), ("bearer" = [])),
)]
async fn metrics_handler(State(state): State<AppState>) -> Response {
    let encoder = TextEncoder::new();
    let metric_families = state.registry.gather();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to encode metrics: {e}"),
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, encoder.format_type())],
        buffer,
    )
        .into_response()
}

/// `GET /health` — simple liveness probe.
///
/// Returns 200 OK if the exporter is running.
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Exporter is alive"),
        (status = 429, description = "Rate limit exceeded",
            headers(("Retry-After" = u64, description = "Seconds until the rate-limit window resets"))),
    ),
)]
async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// `GET /ready` — readiness probe.
#[utoipa::path(get, path = "/ready", tag = "health",
    responses((status = 200, description = "Ready"), (status = 503, description = "Not ready"),
        (status = 429, description = "Rate limit exceeded",
            headers(("Retry-After" = u64, description = "Seconds until the rate-limit window resets")))))]
async fn ready_handler(State(state): State<AppState>) -> impl IntoResponse {
    // Readiness is intentionally independent of router_up. A single failing
    // contract must not remove the exporter from service for healthy contracts.
    let metric_families = state.registry.gather();

    let metric_value = |name: &str| {
        metric_families
            .iter()
            .find(|mf| mf.get_name() == name && mf.get_field_type() == MetricType::GAUGE)
            .and_then(|mf| mf.get_metric().first())
            .map(|m| m.get_gauge().get_value())
            .unwrap_or(0.0)
    };

    let scrape_cycle_completed = metric_value("router_scrape_cycle_completed");
    let rpc_up = metric_value("router_rpc_up");

    if scrape_cycle_completed >= 1.0 && rpc_up >= 1.0 {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::util::ServiceExt; // for `oneshot`

    fn make_app() -> Router {
        let registry = Registry::new();
        // Register a test gauge so the /metrics output is non-empty
        let gauge = prometheus::Gauge::new("test_gauge", "a test gauge").unwrap();
        registry.register(Box::new(gauge.clone())).unwrap();
        gauge.set(42.0);

        let state = AppState { registry };
        Router::new()
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .route("/metrics", get(metrics_handler))
            .route("/health", get(health_handler))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_health_returns_200() {
        let app = make_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics_returns_200_with_prometheus_content_type() {
        let app = make_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            content_type.contains("text/plain"),
            "expected text/plain content type, got: {content_type}"
        );
    }

    #[tokio::test]
    async fn test_metrics_body_contains_gauge() {
        use axum::body::to_bytes;

        let app = make_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body_str = std::str::from_utf8(&body).unwrap();
        assert!(
            body_str.contains("test_gauge"),
            "expected test_gauge in metrics output"
        );
        assert!(
            body_str.contains("42"),
            "expected gauge value 42 in metrics output"
        );
    }

    #[tokio::test]
    async fn test_ready_returns_503_when_router_up_is_zero() {
        let registry = Registry::new();
        // Aggregate router_up may be zero because one contract failed, but
        // this test remains not-ready because no successful cycle is present.
        let gauge = prometheus::Gauge::new("router_up", "exporter health").unwrap();
        registry.register(Box::new(gauge.clone())).unwrap();
        gauge.set(0.0);
        let cycle = prometheus::Gauge::new("router_scrape_cycle_completed", "cycle").unwrap();
        registry.register(Box::new(cycle)).unwrap();
        let rpc = prometheus::Gauge::new("router_rpc_up", "rpc").unwrap();
        registry.register(Box::new(rpc.clone())).unwrap();
        rpc.set(1.0);

        let state = AppState { registry };
        let app = Router::new()
            .route("/ready", get(ready_handler))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_request_id_middleware_sets_response_header() {
        let registry = Registry::new();
        let state = AppState { registry };
        let app = Router::new()
            .route("/health", get(health_handler))
            .layer(middleware::from_fn(request_id_middleware))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let request_id = response
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            !request_id.is_empty(),
            "expected non-empty x-request-id header"
        );
    }

    #[tokio::test]
    async fn test_ready_returns_200_when_router_up_is_one() {
        let registry = Registry::new();
        // Aggregate router_up is not part of the readiness decision, but is
        // retained here to ensure dashboards remain compatible.
        let gauge = prometheus::Gauge::new("router_up", "exporter health").unwrap();
        registry.register(Box::new(gauge.clone())).unwrap();
        gauge.set(1.0);
        let cycle = prometheus::Gauge::new("router_scrape_cycle_completed", "cycle").unwrap();
        registry.register(Box::new(cycle.clone())).unwrap();
        cycle.set(1.0);
        let rpc = prometheus::Gauge::new("router_rpc_up", "rpc").unwrap();
        registry.register(Box::new(rpc.clone())).unwrap();
        rpc.set(1.0);

        let state = AppState { registry };
        let app = Router::new()
            .route("/ready", get(ready_handler))
            .with_state(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// Builds the same route topology as `serve()` — `/metrics` behind
    /// `route_layer`'d auth + replay-protection middleware, `/health` and
    /// `/ready` registered afterward so they're unaffected by either.
    fn make_app_with_auth_and_replay_protection() -> Router {
        let registry = Registry::new();
        let gauge = prometheus::Gauge::new("test_gauge", "a test gauge").unwrap();
        registry.register(Box::new(gauge.clone())).unwrap();
        gauge.set(42.0);
        let state = AppState { registry };

        let auth_config = crate::auth::AuthConfig {
            enabled: true,
            api_key: Some("secret-key".to_string()),
        };
        let replay_config = ReplayProtectionConfig {
            enabled: true,
            cache_size: 10_000,
            nonce_ttl_secs: 3600,
        };
        let nonce_cache = NonceCache::new(replay_config);

        Router::new()
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .route("/metrics", get(metrics_handler))
            .route_layer(middleware::from_fn_with_state(
                nonce_cache,
                crate::replay_protection::replay_protection_middleware,
            ))
            .route_layer(middleware::from_fn_with_state(
                auth_config,
                crate::auth::auth_middleware,
            ))
            .route("/health", get(health_handler))
            .route("/ready", get(ready_handler))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_health_returns_200_without_credentials_when_auth_and_replay_enabled() {
        let app = make_app_with_auth_and_replay_protection();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_ready_returns_200_without_credentials_when_auth_and_replay_enabled() {
        // `/ready` reports `router_up` from the registry; the gauge isn't
        // set here, so it's expected to come back "not ready" (503) rather
        // than erroring out on missing auth/nonce headers (401/400).
        let app = make_app_with_auth_and_replay_protection();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_metrics_still_requires_api_key_when_auth_enabled() {
        let app = make_app_with_auth_and_replay_protection();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_metrics_succeeds_with_valid_api_key_and_nonce() {
        let app = make_app_with_auth_and_replay_protection();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .header("x-api-key", "secret-key")
                    .header("x-nonce", "test-nonce-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_swagger_ui_returns_non_404() {
        let app = make_app();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/swagger-ui/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }
}
