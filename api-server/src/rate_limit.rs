//! Simple per-client rate limiting middleware for the API server.
//!
//! Tracks requests by remote IP address and rejects excess requests with
//! HTTP 429. The client cannot influence its own bucket key: an
//! unauthenticated, client-supplied override (e.g. a header) would let an
//! attacker pick a fresh key per request and dodge the limit entirely, and
//! would also let them grow `buckets` without bound.

use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

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

/// Validates the CLI/env-supplied rate-limit parameters before a
/// [`RateLimiter`] is constructed from them.
///
/// `tokio::time::interval` panics when given a zero-duration period, so a
/// `window_secs` of `0` must be rejected here instead of crashing the
/// background sweeper task (`RateLimiter::spawn_sweeper`) at startup. A
/// `max_requests` of `0` is rejected too, since it would silently reject
/// every single request rather than disabling the limiter.
pub fn validate_rate_limit_args(max_requests: u32, window_secs: u64) -> anyhow::Result<()> {
    if window_secs == 0 {
        anyhow::bail!("rate_limit_window_secs must be greater than 0");
    }
    if max_requests == 0 {
        anyhow::bail!("rate_limit_max_requests must be greater than 0");
    }
    Ok(())
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

    pub fn check_and_remaining(&self, key: &str) -> (bool, u32) {
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
        let allowed = entry.count <= self.config.max_requests;
        (
            allowed,
            self.config.max_requests.saturating_sub(entry.count),
        )
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

    /// Spawns a background task that periodically evicts buckets whose window
    /// has expired, so `buckets` cannot grow without bound over the life of
    /// the process. Must be called from within a Tokio runtime.
    pub fn spawn_sweeper(&self) {
        let buckets = self.buckets.clone();
        let window = self.config.window;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(window);
            loop {
                interval.tick().await;
                let now = Instant::now();
                buckets.retain(|_, entry| now.duration_since(entry.window_start) < window);
            }
        });
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
    req: Request<Body>,
    next: Next,
) -> Response {
    let remote_addr = req
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0)
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)));

    let key = remote_addr.ip().to_string();

    let limiter = &state.rate_limiter;
    let (allowed, remaining) = limiter.check_and_remaining(&key);

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
        let _ = headers.insert(
            "x-rate-limit-reset",
            HeaderValue::from_str(&limiter.retry_after_secs(&key).to_string()).unwrap(),
        );
        response
    } else {
        let retry_after = limiter.retry_after_secs(&key);
        warn!(key = %key, "rate limit exceeded");

        (
            StatusCode::TOO_MANY_REQUESTS,
            [
                (
                    "retry-after",
                    HeaderValue::from_str(&retry_after.to_string()).unwrap(),
                ),
                (
                    "x-rate-limit-limit",
                    HeaderValue::from_str(&limiter.max_requests().to_string()).unwrap(),
                ),
                ("x-rate-limit-remaining", HeaderValue::from_static("0")),
                (
                    "x-rate-limit-reset",
                    HeaderValue::from_str(&retry_after.to_string()).unwrap(),
                ),
            ],
            Json(RateLimitError {
                error: "rate_limit_exceeded",
                message: format!("Too many requests. Retry after {retry_after} second(s)."),
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
        assert!(rl.check_and_remaining("127.0.0.1").0);
        assert!(rl.check_and_remaining("127.0.0.1").0);
        assert!(rl.check_and_remaining("127.0.0.1").0);
    }

    #[test]
    fn rejects_request_over_limit() {
        let rl = limiter(2, 60);
        rl.check_and_remaining("10.0.0.1");
        rl.check_and_remaining("10.0.0.1");
        assert!(!rl.check_and_remaining("10.0.0.1").0);
    }

    #[test]
    fn different_keys_are_independent() {
        let rl = limiter(1, 60);
        assert!(rl.check_and_remaining("192.168.1.1").0);
        assert!(rl.check_and_remaining("192.168.1.2").0);
        assert!(!rl.check_and_remaining("192.168.1.1").0);
    }

    #[test]
    fn valid_rate_limit_args_accepted() {
        assert!(validate_rate_limit_args(100, 60).is_ok());
    }

    #[test]
    fn zero_window_secs_rejected() {
        let err = validate_rate_limit_args(100, 0).unwrap_err();
        assert!(err.to_string().contains("rate_limit_window_secs"));
    }

    #[test]
    fn zero_max_requests_rejected() {
        let err = validate_rate_limit_args(0, 60).unwrap_err();
        assert!(err.to_string().contains("rate_limit_max_requests"));
    }
}
