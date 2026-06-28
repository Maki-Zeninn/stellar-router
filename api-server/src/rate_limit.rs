//! Simple per-client rate limiting middleware for the API server.
//!
//! Tracks requests by `X-Api-Key` header or remote IP and rejects excess
//! requests with HTTP 429.

use std::{net::SocketAddr, sync::Arc, time::{Duration, Instant}};

use axum::{
    body::Body,
    extract::State,
    http::{HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use dashmap::DashMap;
use serde::Serialize;
use tracing::warn;

/// Rate-limit configuration for `/simulate` requests.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub max_requests: u32,
    pub window: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60),
        }
    }
}

#[derive(Debug)]
struct BucketEntry {
    count: u32,
    window_start: Instant,
}

#[derive(Clone, Debug)]
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

    pub fn max_requests(&self) -> u32 {
        self.config.max_requests
    }

    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut entry = self.buckets.entry(key.to_string()).or_insert(BucketEntry {
            count: 0,
            window_start: now,
        });

        if now.duration_since(entry.window_start) >= self.config.window {
            entry.count = 0;
            entry.window_start = now;
        }

        entry.count += 1;
        entry.count <= self.config.max_requests
    }

    pub fn remaining(&self, key: &str) -> u32 {
        if let Some(entry) = self.buckets.get(key) {
            let now = Instant::now();
            if now.duration_since(entry.window_start) >= self.config.window {
                return self.config.max_requests;
            }
            let count = entry.count;
            return self.config.max_requests.saturating_sub(count);
        }
        self.config.max_requests
    }

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

#[derive(Serialize)]
struct RateLimitError {
    error: &'static str,
    message: String,
    retry_after_secs: u64,
}

pub async fn rate_limit_middleware(
    State(state): State<crate::state::AppState>,
    mut req: Request<Body>,
    next: Next<Body>,
) -> Response {
    let remote_addr = req
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0)
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)));

    let key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| remote_addr.ip().to_string());

    let limiter = &state.rate_limiter;
    let allowed = limiter.check(&key);
    let remaining = limiter.remaining(&key);

    if allowed {
        let mut response = next.run(req).await;
        let headers = response.headers_mut();

        let _ = headers.insert(
            "x-rate-limit-limit",
            HeaderValue::from_str(&limiter.max_requests().to_string()).unwrap(),
        );
        let _ = headers.insert(
            "x-rate-limit-remaining",
            HeaderValue::from_str(&remaining.to_string()).unwrap(),
        );
        response
    } else {
        let retry_after = limiter.retry_after_secs(&key);
        warn!(key = %key, "rate limit exceeded");

        (
            StatusCode::TOO_MANY_REQUESTS,
            [(
                "retry-after",
                HeaderValue::from_str(&retry_after.to_string()).unwrap(),
            )],
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
        assert!(rl.check("192.168.1.2"));
        assert!(!rl.check("192.168.1.1"));
    }
}
