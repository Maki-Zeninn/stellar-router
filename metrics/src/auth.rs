//! Request authentication middleware for the metrics exporter.
//!
//! Supports API key-based authentication via:
//! - `Authorization: Bearer <api-key>` header
//! - `X-API-Key: <api-key>` header
//!
//! Configuration via environment variables:
//! - `ROUTER_API_KEY` — API key for authentication (if not set, authentication is disabled)
//! - `ROUTER_AUTH_ENABLED` — Set to "true" to enable authentication (default: false)

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::env;
use subtle::ConstantTimeEq;
use tracing::warn;

/// Authentication configuration.
#[derive(Clone, Debug)]
pub struct AuthConfig {
    /// API key for authentication. If None, authentication is disabled.
    pub api_key: Option<String>,
    /// Whether authentication is enabled.
    pub enabled: bool,
}

impl AuthConfig {
    /// Load authentication configuration from environment variables.
    pub fn from_env() -> Self {
        let enabled = env::var("ROUTER_AUTH_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        let api_key = env::var("ROUTER_API_KEY").ok();

        if enabled && api_key.is_none() {
            warn!("Authentication enabled but ROUTER_API_KEY not set. Authentication will be disabled.");
        }

        if !enabled {
            warn!(
                "Metrics endpoint is unauthenticated — set ROUTER_AUTH_ENABLED=true for production"
            );
        }

        AuthConfig {
            enabled: enabled && api_key.is_some(),
            api_key,
        }
    }
}

/// Authentication middleware that validates API keys.
pub async fn auth_middleware(
    State(config): State<AuthConfig>,
    req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Skip authentication if disabled
    if !config.enabled {
        return Ok(next.run(req).await);
    }

    let headers = req.headers();
    let api_key = extract_api_key(headers);

    match api_key {
        Some(key) => {
            // `config.enabled` is only ever `true` when `from_env` also found an
            // `api_key` (see `AuthConfig::from_env`), so this is guaranteed to be
            // `Some` here — `AuthConfig` has no other public constructor.
            let expected_key = config
                .api_key
                .as_ref()
                .expect("enabled implies api_key is set");
            if constant_time_equals(&key, expected_key) {
                Ok(next.run(req).await)
            } else {
                Err(AuthError::InvalidKey)
            }
        }
        None => Err(AuthError::MissingKey),
    }
}

fn constant_time_equals(lhs: &str, rhs: &str) -> bool {
    lhs.as_bytes().ct_eq(rhs.as_bytes()).into()
}

/// Extract API key from request headers.
fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    // Try Authorization: Bearer <key>
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(key) = auth_str.strip_prefix("Bearer ") {
                return Some(key.to_string());
            }
        }
    }

    // Try X-API-Key: <key>
    if let Some(api_key_header) = headers.get("x-api-key") {
        if let Ok(key) = api_key_header.to_str() {
            return Some(key.to_string());
        }
    }

    None
}

/// Authentication errors.
#[derive(Debug)]
pub enum AuthError {
    /// Missing API key in request.
    MissingKey,
    /// Invalid API key provided.
    InvalidKey,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingKey => (StatusCode::UNAUTHORIZED, "Missing API key"),
            AuthError::InvalidKey => (StatusCode::UNAUTHORIZED, "Invalid API key"),
        };

        (status, message).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::Request as HttpRequest,
        middleware,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    fn test_router(config: AuthConfig) -> Router {
        Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(config, auth_middleware))
    }

    async fn body_string(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn test_middleware_disabled_passes_through() {
        let app = test_router(AuthConfig {
            enabled: false,
            api_key: Some("secret".to_string()),
        });

        let response = app
            .oneshot(HttpRequest::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_string(response).await, "ok");
    }

    #[tokio::test]
    async fn test_middleware_missing_key() {
        let app = test_router(AuthConfig {
            enabled: true,
            api_key: Some("secret".to_string()),
        });

        let response = app
            .oneshot(HttpRequest::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body_string(response).await, "Missing API key");
    }

    #[tokio::test]
    async fn test_middleware_invalid_key() {
        let app = test_router(AuthConfig {
            enabled: true,
            api_key: Some("secret".to_string()),
        });

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/")
                    .header("authorization", "Bearer wrong-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body_string(response).await, "Invalid API key");
    }

    #[tokio::test]
    async fn test_middleware_valid_bearer_key() {
        let app = test_router(AuthConfig {
            enabled: true,
            api_key: Some("secret".to_string()),
        });

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/")
                    .header("authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_string(response).await, "ok");
    }

    #[tokio::test]
    async fn test_middleware_valid_x_api_key() {
        let app = test_router(AuthConfig {
            enabled: true,
            api_key: Some("secret".to_string()),
        });

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/")
                    .header("x-api-key", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_string(response).await, "ok");
    }

    #[test]
    fn test_from_env_enabled_without_key_falls_back_to_disabled() {
        // Serialize env access with other tests via a unique guard is not
        // available here, so save/restore the relevant variables.
        let prev_enabled = env::var("ROUTER_AUTH_ENABLED").ok();
        let prev_key = env::var("ROUTER_API_KEY").ok();

        env::set_var("ROUTER_AUTH_ENABLED", "true");
        env::remove_var("ROUTER_API_KEY");

        let config = AuthConfig::from_env();
        assert!(!config.enabled);
        assert!(config.api_key.is_none());

        match prev_enabled {
            Some(v) => env::set_var("ROUTER_AUTH_ENABLED", v),
            None => env::remove_var("ROUTER_AUTH_ENABLED"),
        }
        match prev_key {
            Some(v) => env::set_var("ROUTER_API_KEY", v),
            None => env::remove_var("ROUTER_API_KEY"),
        }
    }

    #[test]
    fn test_extract_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-key-123".parse().unwrap());

        let key = extract_api_key(&headers);
        assert_eq!(key, Some("test-key-123".to_string()));
    }

    #[test]
    fn test_extract_api_key_header() {
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "test-key-456".parse().unwrap());

        let key = extract_api_key(&headers);
        assert_eq!(key, Some("test-key-456".to_string()));
    }

    #[test]
    fn test_extract_api_key_missing() {
        let headers = HeaderMap::new();
        let key = extract_api_key(&headers);
        assert_eq!(key, None);
    }

    #[test]
    fn test_bearer_token_takes_precedence() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer bearer-key".parse().unwrap());
        headers.insert("x-api-key", "api-key".parse().unwrap());

        let key = extract_api_key(&headers);
        assert_eq!(key, Some("bearer-key".to_string()));
    }

    #[test]
    fn test_constant_time_compare_rejects_prefix_mismatch() {
        assert!(!constant_time_equals("super-secret", "super-guess"));
    }
}
