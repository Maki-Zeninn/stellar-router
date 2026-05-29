//! Token-bucket rate limiter middleware.
//!
//! Limits requests per IP address (or per `X-Api-Key` header when present).
//! Configuration is driven by [`RateLimitConfig`] which can be set via CLI
//! flags or environment variables.
//!
//! Returns HTTP 429 with a JSON body and `Retry-After` header when the limit
//! is exceeded.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use dashmap::DashMap;
use serde::Serialize;
use tracing::warn;

// ── Config ────────────────────────────────────────────────────────────────────

/// Rate-limit configuration (injected via CLI / env).
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests allowed per window.
    pub max_requests: u32,
    /// Duration of the sliding window.
    pub window: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 60,
            window: Duration::from_secs(60),
        }
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct BucketEntry {
    count: u32,
    window_start: Instant,
}

/// Shared rate-limiter state — cheap to clone (Arc inside).
#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: Arc<DashMap<String, BucketEntry>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(DashMap::new()),
        }
    }

    /// Returns `true` if the request is allowed, `false` if it should be
    /// rejected (limit exceeded).
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut entry = self.buckets.entry(key.to_string()).or_insert(BucketEntry {
            count: 0,
            window_start: now,
        });

        // Reset window if expired
        if now.duration_since(entry.window_start) >= self.config.window {
            entry.count = 0;
            entry.window_start = now;
        }

        entry.count += 1;
        entry.count <= self.config.max_requests
    }

    /// Seconds remaining in the current window for a given key.
    pub fn retry_after_secs(&self, key: &str) -> u64 {
        if let Some(entry) = self.buckets.get(key) {
            let elapsed = Instant::now().duration_since(entry.window_start);
            if elapsed < self.config.window {
                return (self.config.window - elapsed).as_secs().max(1);
            }
        }
        1
    }
}

// ── Middleware ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct RateLimitError {
    error: &'static str,
    message: String,
    retry_after_secs: u64,
}

/// Axum middleware that enforces per-IP (or per-API-key) rate limits.
pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Prefer X-Api-Key as the rate-limit key; fall back to remote IP.
    let key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| addr.ip().to_string());

    if limiter.check(&key) {
        next.run(req).await
    } else {
        let retry_after = limiter.retry_after_secs(&key);
        warn!(key = %key, "rate limit exceeded");
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", retry_after.to_string())],
            Json(RateLimitError {
                error: "rate_limit_exceeded",
                message: format!(
                    "Too many requests. Retry after {} second(s).",
                    retry_after
                ),
                retry_after_secs: retry_after,
            }),
        )
            .into_response()
    }
}

// ── CLI args extension ────────────────────────────────────────────────────────

/// Parse rate-limit config from environment / defaults.
/// Extend [`crate::cli::Args`] with these fields to make them configurable.
pub fn config_from_env() -> RateLimitConfig {
    let max_requests = std::env::var("ROUTER_RATE_LIMIT_MAX_REQUESTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60u32);

    let window_secs = std::env::var("ROUTER_RATE_LIMIT_WINDOW_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60u64);

    RateLimitConfig {
        max_requests,
        window: Duration::from_secs(window_secs),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter(max: u32, window_secs: u64) -> RateLimiter {
        RateLimiter::new(RateLimitConfig {
            max_requests: max,
            window: Duration::from_secs(window_secs),
        })
    }

    #[test]
    fn allows_requests_within_limit() {
        let rl = limiter(3, 60);
        assert!(rl.check("127.0.0.1"));
        assert!(rl.check("127.0.0.1"));
        assert!(rl.check("127.0.0.1"));
    }

    #[test]
    fn rejects_request_over_limit() {
        let rl = limiter(2, 60);
        rl.check("10.0.0.1");
        rl.check("10.0.0.1");
        assert!(!rl.check("10.0.0.1"));
    }

    #[test]
    fn different_keys_are_independent() {
        let rl = limiter(1, 60);
        assert!(rl.check("192.168.1.1"));
        assert!(rl.check("192.168.1.2")); // different key — should pass
        assert!(!rl.check("192.168.1.1")); // same key — should fail
    }
}
