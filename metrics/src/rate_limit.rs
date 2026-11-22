//! Fixed-window rate limiter middleware.
//!
//! Limits requests per IP address with a fixed-window counter: each key may
//! make up to `max_requests` requests per `window`, and the counter resets to
//! zero once `window` has elapsed since the window started. Because the
//! windows are fixed rather than sliding, a client can send up to
//! `2 * max_requests` requests in a short burst straddling a window boundary
//! (the end of one window plus the start of the next).
//!
//! The client cannot influence its own
//! bucket key: an unauthenticated, client-supplied override (e.g. a header)
//! would let an attacker pick a fresh key per request and dodge the limit
//! entirely, and would also let them evict every other client's bucket
//! once `max_buckets` is exceeded. Configuration is driven by
//! [`RateLimitConfig`] which can be set via CLI flags or environment
//! variables.
//!
//! Returns HTTP 429 with a JSON body and `Retry-After` header when the limit
//! is exceeded.

use std::{
    collections::VecDeque,
    net::SocketAddr,
    sync::{Arc, Mutex},
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
    /// Duration of each fixed window. The request count resets once this much
    /// time has passed since the window started, so up to `2 * max_requests`
    /// requests can land in a short burst across a window boundary.
    pub window: Duration,
    /// Maximum number of tracked buckets to retain in memory.
    pub max_buckets: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 60,
            window: Duration::from_secs(60),
            max_buckets: 1024,
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
    eviction_order: Arc<Mutex<VecDeque<String>>>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config: RateLimitConfig {
                max_buckets: config.max_buckets.max(1),
                ..config
            },
            buckets: Arc::new(DashMap::new()),
            eviction_order: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Returns `true` if the request is allowed, `false` if it should be
    /// rejected (limit exceeded).
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let key_owned = key.to_string();
        let mut inserted = false;

        let should_allow = {
            // The entry API holds the shard lock from lookup through update, so
            // two concurrent first requests for a key cannot both create the
            // bucket (the second would reset `count`) or both queue it below.
            let mut entry = self.buckets.entry(key_owned.clone()).or_insert_with(|| {
                inserted = true;
                BucketEntry {
                    count: 0,
                    window_start: now,
                }
            });

            // Reset window if expired
            if now.duration_since(entry.window_start) >= self.config.window {
                entry.count = 0;
                entry.window_start = now;
            }

            entry.count += 1;
            entry.count <= self.config.max_requests
        };

        if inserted {
            let mut order = self.eviction_order.lock().unwrap();
            order.push_back(key_owned.clone());
            while order.len() > self.config.max_buckets {
                if let Some(oldest) = order.pop_front() {
                    self.buckets.remove(&oldest);
                } else {
                    break;
                }
            }
        }

        should_allow
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

/// The rate-limit bucket key for a request: always the remote IP.
///
/// This must not be influenced by anything the client supplies (such as an
/// `X-Api-Key` header) — otherwise a client can dodge the limit by rotating
/// the header per request, and can also evict every other client's bucket
/// once `max_buckets` is exceeded (see `RateLimiter::check`). `req` is
/// intentionally unused: it's kept in the signature so callers (and tests)
/// can't accidentally key on anything from it.
fn bucket_key(_req: &Request<Body>, addr: SocketAddr) -> String {
    addr.ip().to_string()
}

/// Axum middleware that enforces per-IP rate limits.
pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let key = bucket_key(&req, addr);

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
                message: format!("Too many requests. Retry after {retry_after} second(s)."),
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

    // Issue #1200: max_buckets was hardcoded here even though it's a regular
    // constructor field (used by RateLimiter::check's eviction logic to
    // bound memory), not a private constant — every deployment got exactly
    // 1024 tracked buckets regardless of configuration.
    let max_buckets = std::env::var("ROUTER_RATE_LIMIT_MAX_BUCKETS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024usize);

    RateLimitConfig {
        max_requests,
        window: Duration::from_secs(window_secs),
        max_buckets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter(max: u32, window_secs: u64) -> RateLimiter {
        RateLimiter::new(RateLimitConfig {
            max_requests: max,
            window: Duration::from_secs(window_secs),
            max_buckets: 1024,
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
    fn retry_after_secs_reflects_remaining_window() {
        let rl = limiter(1, 60);
        assert!(rl.check("1.2.3.4"));
        let retry_after = rl.retry_after_secs("1.2.3.4");
        assert!((1..=60).contains(&retry_after));
    }

    #[test]
    fn retry_after_secs_defaults_to_one_for_unknown_key() {
        let rl = limiter(1, 60);
        assert_eq!(rl.retry_after_secs("never-seen"), 1);
    }

    #[test]
    fn different_keys_are_independent() {
        let rl = limiter(1, 60);
        assert!(rl.check("192.168.1.1"));
        assert!(rl.check("192.168.1.2")); // different key — should pass
        assert!(!rl.check("192.168.1.1")); // same key — should fail
    }

    #[test]
    fn concurrent_first_requests_create_one_bucket() {
        const THREADS: u32 = 16;
        let rl = limiter(1_000, 60);

        std::thread::scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    rl.check("203.0.113.50");
                });
            }
        });

        let queued = rl
            .eviction_order
            .lock()
            .unwrap()
            .iter()
            .filter(|k| k.as_str() == "203.0.113.50")
            .count();
        assert_eq!(queued, 1, "key must be queued for eviction exactly once");
        assert_eq!(
            rl.buckets.get("203.0.113.50").unwrap().count,
            THREADS,
            "every request must be counted"
        );
    }

    #[test]
    fn evicts_oldest_entries_when_max_buckets_is_exceeded() {
        let rl = RateLimiter::new(RateLimitConfig {
            max_requests: 10,
            window: Duration::from_secs(60),
            max_buckets: 2,
        });

        assert!(rl.check("one"));
        assert!(rl.check("two"));
        assert!(rl.check("three"));

        assert!(rl.buckets.contains_key("three"));
        assert!(!rl.buckets.contains_key("one"));
    }

    // ── Issue #1432: bucket key must not be influenced by X-Api-Key ─────────

    #[test]
    fn bucket_key_ignores_x_api_key_header() {
        let addr: SocketAddr = "203.0.113.7:4321".parse().unwrap();

        let req_a = Request::builder()
            .header("x-api-key", "client-a-key")
            .body(Body::empty())
            .unwrap();
        let req_b = Request::builder()
            .header("x-api-key", "client-b-key")
            .body(Body::empty())
            .unwrap();
        let req_none = Request::builder().body(Body::empty()).unwrap();

        let key_a = bucket_key(&req_a, addr);
        let key_b = bucket_key(&req_b, addr);
        let key_none = bucket_key(&req_none, addr);

        assert_eq!(key_a, key_b);
        assert_eq!(key_a, key_none);
        assert_eq!(key_a, "203.0.113.7");
    }

    #[test]
    fn same_ip_different_api_keys_share_one_bucket() {
        let rl = limiter(1, 60);
        let addr: SocketAddr = "198.51.100.9:1234".parse().unwrap();

        let req_a = Request::builder()
            .header("x-api-key", "rotating-key-1")
            .body(Body::empty())
            .unwrap();
        let req_b = Request::builder()
            .header("x-api-key", "rotating-key-2")
            .body(Body::empty())
            .unwrap();

        assert!(rl.check(&bucket_key(&req_a, addr)));
        // Same IP, different X-Api-Key: must hit the same bucket and be
        // rejected, not bypass the limit with a "fresh" key.
        assert!(!rl.check(&bucket_key(&req_b, addr)));
    }

    // config_from_env reads process-global env vars, so tests that touch them
    // must run serially to avoid racing under cargo test's parallel runner.
    static ENV_GUARD: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();

    fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
        let guard = ENV_GUARD.get_or_init(|| std::sync::Mutex::new(()));
        let _lock = guard.lock().unwrap_or_else(|e| e.into_inner());
        f()
    }

    // Safety: guarded by `with_env_lock` above, so no other thread in this
    // process observes or mutates these env vars concurrently.
    fn set_env(key: &str, value: &str) {
        unsafe { std::env::set_var(key, value) };
    }

    fn remove_env(key: &str) {
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn config_from_env_uses_defaults_when_unset() {
        with_env_lock(|| {
            remove_env("ROUTER_RATE_LIMIT_MAX_REQUESTS");
            remove_env("ROUTER_RATE_LIMIT_WINDOW_SECS");

            let config = config_from_env();
            assert_eq!(config.max_requests, 60);
            assert_eq!(config.window, Duration::from_secs(60));
            assert_eq!(config.max_buckets, 1024);
        });
    }

    #[test]
    fn config_from_env_parses_values_from_env() {
        with_env_lock(|| {
            set_env("ROUTER_RATE_LIMIT_MAX_REQUESTS", "5");
            set_env("ROUTER_RATE_LIMIT_WINDOW_SECS", "30");

            let config = config_from_env();
            assert_eq!(config.max_requests, 5);
            assert_eq!(config.window, Duration::from_secs(30));

            remove_env("ROUTER_RATE_LIMIT_MAX_REQUESTS");
            remove_env("ROUTER_RATE_LIMIT_WINDOW_SECS");
        });
    }

    #[test]
    fn config_from_env_falls_back_to_default_on_unparsable_value() {
        with_env_lock(|| {
            set_env("ROUTER_RATE_LIMIT_MAX_REQUESTS", "not-a-number");
            set_env("ROUTER_RATE_LIMIT_WINDOW_SECS", "also-not-a-number");

            let config = config_from_env();
            assert_eq!(config.max_requests, 60);
            assert_eq!(config.window, Duration::from_secs(60));

            remove_env("ROUTER_RATE_LIMIT_MAX_REQUESTS");
            remove_env("ROUTER_RATE_LIMIT_WINDOW_SECS");
        });
    }
}
