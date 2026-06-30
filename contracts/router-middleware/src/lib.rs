#![no_std]

//! # router-middleware
//!
//! Pre/post call hook middleware for the stellar-router suite.
//! Supports rate limiting, call logging, and per-route fee configuration.
//!
//! ## Features
//! - Per-caller rate limiting (max calls per time window)
//! - Call event logging with timestamps
//! - Configurable per-route fees
//! - Admin-controlled hook enable/disable
//!
//! ## Events (following naming convention: past tense verbs in snake_case)
//! - `pre_call` — Pre-call validation hook executed
//! - `post_call` — Post-call hook executed
//! - `circuit_opened` — Circuit breaker opened for route
//! - `circuit_closed` — Circuit breaker closed after successful recovery
//! - `middleware_enabled` — Global middleware enabled/disabled
//! - `call_log_cleared` — Call log cleared for route
//! - `admin_transferred` — Admin transferred to new address

pub mod call_log;
pub mod circuit_breaker;
pub mod rate_limit;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, Map, String, Symbol, Vec,
};

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Admin,
    RouteCallState(String), // route_name -> RouteCallState
    RouteConfig(String),    // route_name -> RouteConfig
    GlobalEnabled,
    TotalCalls,
    CallLog(String),           // route_name -> CallLogState
    ConfiguredRoutes,          // Vec<String>
    CallLogSummary(String),    // route_name -> CallLogSummary
    RateLimitStrategy(String), // route_name -> RateLimitStrategy
    CallerRateLimit(String, Address), // (route, caller) -> CallerRateLimitConfig
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RateLimitState {
    /// Number of calls in current window
    pub calls_in_window: u32,
    /// Timestamp when window started
    pub window_start: u64,
    /// Total number of times rate limit was exceeded
    pub total_violations: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RouteRateLimitStats {
    /// Total number of calls across all callers in current window
    pub total_calls_in_window: u32,
    /// Timestamp when the current window started (earliest window start among all callers)
    pub window_start: u64,
    /// Total number of rate limit violations across all callers
    pub total_violations: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RouteConfig {
    /// Max calls per window (0 = unlimited)
    pub max_calls_per_window: u32,
    /// Window size in seconds
    pub window_seconds: u64,
    /// Whether this route is enabled
    pub enabled: bool,
    /// Circuit breaker failure threshold (0 = disabled)
    pub failure_threshold: u32,
    /// Circuit breaker recovery window in seconds
    pub recovery_window_seconds: u64,
    /// Max call log entries to keep (0 = disabled)
    pub log_retention: u32,
    /// Extra calls allowed above max_calls_per_window before rejection (0 = no burst)
    pub burst_allowance: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CircuitBreakerState {
    /// Number of consecutive failures
    pub failure_count: u32,
    /// Timestamp when circuit was opened
    pub opened_at: u64,
    /// Whether circuit is currently open
    pub is_open: bool,
    /// Whether circuit is in half-open state (probe mode)
    pub is_half_open: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RouteCallState {
    /// Per-caller rate limit state for the route
    pub rate_limits: Map<Address, RateLimitState>,
    /// Route-level circuit breaker state
    pub circuit_breaker: CircuitBreakerState,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CallLogEntry {
    /// The caller address
    pub caller: Address,
    /// Timestamp of the call
    pub timestamp: u64,
    /// Whether the call succeeded
    pub success: bool,
    /// The route that was called
    pub route: String,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CallLogState {
    /// Fixed-capacity call entries retained for the route.
    /// Pre-allocated in `configure_route` to `log_retention` with placeholder entries.
    pub entries: Vec<CallLogEntry>,
    /// Index of the oldest entry in `entries` (0 when not wrapped)
    pub head: u32,
    /// Total real entries written so far, capped at entries.len()
    pub count: u32,
}

/// Aggregated summary for a route's call log.
/// Maintained incrementally to avoid loading all entries.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CallLogSummary {
    pub total_calls: u32,
    pub success_count: u32,
    pub failure_count: u32,
    pub last_call_timestamp: u64,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum MiddlewareError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    RateLimitExceeded = 4,
    RouteDisabled = 5,
    MiddlewareDisabled = 6,
    InvalidConfig = 7,
    CircuitOpen = 8,
}

/// Per-caller rate limit override for a specific route.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CallerRateLimitConfig {
    pub max_calls: u32,
    pub window_secs: u64,
}

/// Configurable strategy for handling rate limit exceeded events.
#[contracttype]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RateLimitStrategy {
    /// Return an error immediately (default).
    Reject,
    /// Allow the call but emit a warning event and increment a throttle counter.
    Throttle,
    /// Allow the call and log that the limit was exceeded (soft enforcement).
    LogOnly,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RouterMiddleware;

#[contractimpl]
impl RouterMiddleware {
    const MAX_LOG_RETENTION: u32 = 10_000;

    /// Initialize middleware with an admin.
    pub fn initialize(env: Env, admin: Address) -> Result<(), MiddlewareError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(MiddlewareError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::GlobalEnabled, &true);
        env.storage().instance().set(&DataKey::TotalCalls, &0u64);
        Ok(())
    }

    /// Configure a route's middleware settings.
    pub fn configure_route(
        env: Env,
        caller: Address,
        route: String,
        max_calls_per_window: u32,
        window_seconds: u64,
        enabled: bool,
        failure_threshold: u32,
        recovery_window_seconds: u64,
        log_retention: u32,
        burst_allowance: u32,
    ) -> Result<(), MiddlewareError> {
        caller.require_auth();
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, MiddlewareError)?;

        if window_seconds == 0 && max_calls_per_window > 0 {
            return Err(MiddlewareError::InvalidConfig);
        }

        if log_retention > Self::MAX_LOG_RETENTION {
            return Err(MiddlewareError::InvalidConfig);
        }

        let config = RouteConfig {
            max_calls_per_window,
            window_seconds,
            enabled,
            failure_threshold,
            recovery_window_seconds,
            log_retention,
            burst_allowance,
        };
        env.storage()
            .instance()
            .set(&DataKey::RouteConfig(route.clone()), &config);

        let mut configured: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::ConfiguredRoutes)
            .unwrap_or_else(|| Vec::new(&env));
        if !configured.contains(&route) {
            configured.push_back(route.clone());
            env.storage()
                .instance()
                .set(&DataKey::ConfiguredRoutes, &configured);
        }

        if log_retention > 0 {
            env.storage()
                .instance()
                .remove(&DataKey::CallLog(route.clone()));

            let placeholder = CallLogEntry {
                caller: caller.clone(),
                timestamp: 0,
                success: false,
                route: route.clone(),
            };
            let mut entries = Vec::new(&env);
            for _ in 0..log_retention {
                entries.push_back(placeholder.clone());
            }
            let log = CallLogState {
                entries,
                head: 0,
                count: 0,
            };
            env.storage()
                .instance()
                .set(&DataKey::CallLog(route.clone()), &log);
        } else {
            env.storage()
                .instance()
                .remove(&DataKey::CallLog(route.clone()));
        }

        Ok(())
    }

    /// Pre-call hook: validates rate limits and route status.
    pub fn pre_call(env: Env, caller: Address, route: String) -> Result<(), MiddlewareError> {
        let enabled: bool = env
            .storage()
            .instance()
            .get(&DataKey::GlobalEnabled)
            .unwrap_or(true);
        if !enabled {
            return Err(MiddlewareError::MiddlewareDisabled);
        }

        let new_route_call_state = if let Some(config) = env
            .storage()
            .instance()
            .get::<DataKey, RouteConfig>(&DataKey::RouteConfig(route.clone()))
        {
            let mut route_call_state: RouteCallState = env
                .storage()
                .instance()
                .get(&DataKey::RouteCallState(route.clone()))
                .unwrap_or(RouteCallState {
                    rate_limits: Map::new(&env),
                    circuit_breaker: CircuitBreakerState {
                        failure_count: 0,
                        opened_at: 0,
                        is_open: false,
                        is_half_open: false,
                    },
                });

            if !config.enabled {
                return Err(MiddlewareError::RouteDisabled);
            }

            let mut state_changed = false;
            if config.failure_threshold > 0 {
                if route_call_state.circuit_breaker.is_open {
                    let now = env.ledger().timestamp();
                    let recovers = config.recovery_window_seconds > 0
                        && now
                            >= route_call_state.circuit_breaker.opened_at
                                + config.recovery_window_seconds;
                    if !recovers {
                        return Err(MiddlewareError::CircuitOpen);
                    }
                    route_call_state.circuit_breaker.is_open = false;
                    route_call_state.circuit_breaker.is_half_open = true;
                    state_changed = true;
                }
            }

            if config.max_calls_per_window > 0 {
                let now = env.ledger().timestamp();
                let state: RateLimitState = route_call_state
                    .rate_limits
                    .get(caller.clone())
                    .unwrap_or(RateLimitState {
                        calls_in_window: 0,
                        window_start: now,
                        total_violations: 0,
                    });

                // Resolve effective limit: per-caller override takes precedence over
                // the route-level default when a CallerRateLimit entry is present.
                let (effective_limit, effective_window) = env
                    .storage()
                    .instance()
                    .get::<DataKey, CallerRateLimitConfig>(&DataKey::CallerRateLimit(
                        route.clone(),
                        caller.clone(),
                    ))
                    .map(|c| (c.max_calls, c.window_secs))
                    .unwrap_or((config.max_calls_per_window, config.window_seconds));

                let window_elapsed = now >= state.window_start + effective_window;
                let calls = if window_elapsed { 0 } else { state.calls_in_window };
                let window_start = if window_elapsed { now } else { state.window_start };

                if calls >= effective_limit {
                    route_call_state.rate_limits.set(
                        caller.clone(),
                        RateLimitState {
                            calls_in_window: calls,
                            window_start,
                            total_violations: state.total_violations + 1,
                        },
                    );

                    let strategy: RateLimitStrategy = env
                        .storage()
                        .instance()
                        .get(&DataKey::RateLimitStrategy(route.clone()))
                        .unwrap_or(RateLimitStrategy::Reject);

                    match strategy {
                        RateLimitStrategy::Reject => {
                            env.storage()
                                .instance()
                                .set(&DataKey::RouteCallState(route.clone()), &route_call_state);
                            return Err(MiddlewareError::RateLimitExceeded);
                        }
                        RateLimitStrategy::Throttle => {
                            env.events().publish(
                                (Symbol::new(&env, router_common::EVENT_RATE_LIMIT_THROTTLED),),
                                (caller.clone(), route.clone()),
                            );
                            state_changed = true;
                        }
                        RateLimitStrategy::LogOnly => {
                            env.events().publish(
                                (Symbol::new(&env, router_common::EVENT_RATE_LIMIT_EXCEEDED),),
                                (caller.clone(), route.clone()),
                            );
                            state_changed = true;
                        }
                    }
                } else {
                    route_call_state.rate_limits.set(
                        caller.clone(),
                        RateLimitState {
                            calls_in_window: calls + 1,
                            window_start,
                            total_violations: state.total_violations,
                        },
                    );
                    state_changed = true;
                }
            }

            if state_changed { Some(route_call_state) } else { None }
        } else {
            None
        };

        let still_enabled: bool = env
            .storage()
            .instance()
            .get(&DataKey::GlobalEnabled)
            .unwrap_or(true);
        if !still_enabled {
            return Err(MiddlewareError::MiddlewareDisabled);
        }

        if let Some(config) = env
            .storage()
            .instance()
            .get::<DataKey, RouteConfig>(&DataKey::RouteConfig(route.clone()))
        {
            if !config.enabled {
                return Err(MiddlewareError::RouteDisabled);
            }
        }

        if let Some(route_call_state) = new_route_call_state {
            env.storage()
                .instance()
                .set(&DataKey::RouteCallState(route.clone()), &route_call_state);
        }

        let total: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalCalls)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::TotalCalls, &(total + 1));

        env.events().publish(
            (Symbol::new(&env, "pre_call"),),
            (caller.clone(), route.clone()),
        );

        Ok(())
    }

    /// Post-call hook: tracks failures and manages circuit breaker.
    pub fn post_call(env: Env, caller: Address, route: String, success: bool) {
        env.events().publish(
            (Symbol::new(&env, "post_call"),),
            (caller.clone(), route.clone(), success),
        );

        if let Some(config) = env
            .storage()
            .instance()
            .get::<DataKey, RouteConfig>(&DataKey::RouteConfig(route.clone()))
        {
            if config.log_retention > 0 {
                let mut log: CallLogState = env
                    .storage()
                    .instance()
                    .get(&DataKey::CallLog(route.clone()))
                    .unwrap_or(CallLogState {
                        entries: Vec::new(&env),
                        head: 0,
                        count: 0,
                    });

                let entry = CallLogEntry {
                    caller: caller.clone(),
                    timestamp: env.ledger().timestamp(),
                    success,
                    route: route.clone(),
                };

                let cap = config.log_retention;
                let len = log.entries.len();
                if len < cap {
                    log.entries.push_back(entry);
                } else {
                    log.entries.set(log.head, entry);
                    log.head = (log.head + 1) % cap;
                }
                log.count = log.count.saturating_add(1).min(cap);

                env.storage()
                    .instance()
                    .set(&DataKey::CallLog(route.clone()), &log);

                let mut summary: CallLogSummary = env
                    .storage()
                    .instance()
                    .get(&DataKey::CallLogSummary(route.clone()))
                    .unwrap_or(CallLogSummary {
                        total_calls: 0,
                        success_count: 0,
                        failure_count: 0,
                        last_call_timestamp: 0,
                    });
                summary.total_calls += 1;
                if success {
                    summary.success_count += 1;
                } else {
                    summary.failure_count += 1;
                }
                summary.last_call_timestamp = env.ledger().timestamp();
                env.storage()
                    .instance()
                    .set(&DataKey::CallLogSummary(route.clone()), &summary);
            }
        }

        if !success {
            if let Some(config) = env
                .storage()
                .instance()
                .get::<DataKey, RouteConfig>(&DataKey::RouteConfig(route.clone()))
            {
                if config.failure_threshold > 0 {
                    let mut route_call_state: RouteCallState = env
                        .storage()
                        .instance()
                        .get(&DataKey::RouteCallState(route.clone()))
                        .unwrap_or(RouteCallState {
                            rate_limits: Map::new(&env),
                            circuit_breaker: CircuitBreakerState {
                                failure_count: 0,
                                opened_at: 0,
                                is_open: false,
                                is_half_open: false,
                            },
                        });

                    if route_call_state.circuit_breaker.is_half_open {
                        route_call_state.circuit_breaker.is_half_open = false;
                        route_call_state.circuit_breaker.is_open = true;
                        route_call_state.circuit_breaker.opened_at = env.ledger().timestamp();
                        route_call_state.circuit_breaker.failure_count = 1;
                        env.events().publish(
                            (Symbol::new(&env, "circuit_opened"),),
                            (
                                route.clone(),
                                route_call_state.circuit_breaker.failure_count,
                            ),
                        );
                    } else {
                        route_call_state.circuit_breaker.failure_count += 1;

                        if route_call_state.circuit_breaker.failure_count
                            >= config.failure_threshold
                        {
                            route_call_state.circuit_breaker.is_open = true;
                            route_call_state.circuit_breaker.opened_at = env.ledger().timestamp();
                            env.events().publish(
                                (Symbol::new(&env, "circuit_opened"),),
                                (
                                    route.clone(),
                                    route_call_state.circuit_breaker.failure_count,
                                ),
                            );
                        }
                    }

                    env.storage()
                        .instance()
                        .set(&DataKey::RouteCallState(route), &route_call_state);
                }
            }
        } else {
            if let Some(config) = env
                .storage()
                .instance()
                .get::<DataKey, RouteConfig>(&DataKey::RouteConfig(route.clone()))
            {
                if config.failure_threshold > 0 {
                    let mut route_call_state: RouteCallState = env
                        .storage()
                        .instance()
                        .get(&DataKey::RouteCallState(route.clone()))
                        .unwrap_or(RouteCallState {
                            rate_limits: Map::new(&env),
                            circuit_breaker: CircuitBreakerState {
                                failure_count: 0,
                                opened_at: 0,
                                is_open: false,
                                is_half_open: false,
                            },
                        });

                    if route_call_state.circuit_breaker.is_half_open {
                        route_call_state.circuit_breaker.is_half_open = false;
                        route_call_state.circuit_breaker.failure_count = 0;
                        route_call_state.circuit_breaker.opened_at = 0;
                        env.events().publish(
                            (Symbol::new(&env, router_common::EVENT_CIRCUIT_CLOSED),),
                            route.clone(),
                        );
                    } else if !route_call_state.circuit_breaker.is_open
                        && route_call_state.circuit_breaker.failure_count > 0
                    {
                        route_call_state.circuit_breaker.failure_count = 0;
                    }

                    env.storage()
                        .instance()
                        .set(&DataKey::RouteCallState(route), &route_call_state);
                }
            }
        }
    }

    /// Enable or disable all middleware globally.
    pub fn set_global_enabled(
        env: Env,
        caller: Address,
        enabled: bool,
    ) -> Result<(), MiddlewareError> {
        caller.require_auth();
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, MiddlewareError)?;
        env.storage()
            .instance()
            .set(&DataKey::GlobalEnabled, &enabled);
        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_MIDDLEWARE_ENABLED),),
            enabled,
        );
        Ok(())
    }

    /// Get total calls processed.
    pub fn total_calls(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::TotalCalls)
            .unwrap_or(0)
    }

    /// Get the call log for a route.
    pub fn get_call_log(env: Env, route: String) -> Vec<CallLogEntry> {
        Self::get_call_log_internal(&env, route, None)
    }

    /// Get a filtered call log for a route.
    pub fn get_call_log_filtered(env: Env, route: String, success_only: bool) -> Vec<CallLogEntry> {
        Self::get_call_log_internal(&env, route, Some(success_only))
    }

    /// Private internal helper to deduplicate ring buffer traversal and filtering logic.
    fn get_call_log_internal(
        env: &Env,
        route: String,
        success_filter: Option<bool>,
    ) -> Vec<CallLogEntry> {
        let Some(log_state) = env
            .storage()
            .instance()
            .get::<DataKey, CallLogState>(&DataKey::CallLog(route))
        else {
            return Vec::new(env);
        };

        let count = log_state.count;
        if count == 0 {
            return Vec::new(env);
        }

        let cap = log_state.entries.len();
        if cap == 0 {
            return Vec::new(env);
        }

        let mut ordered = Vec::new(env);
        if count < cap {
            for i in 0..count {
                if let Some(entry) = log_state.entries.get(i) {
                    let should_include = match success_filter {
                        None => true,
                        Some(status) => entry.success == status,
                    };
                    if should_include {
                        ordered.push_back(entry);
                    }
                }
            }
        } else {
            for i in 0..cap {
                let idx = (log_state.head + i) % (cap as u32);
                if let Some(entry) = log_state.entries.get(idx) {
                    let should_include = match success_filter {
                        None => true,
                        Some(status) => entry.success == status,
                    };
                    if should_include {
                        ordered.push_back(entry);
                    }
                }
            }
        }
        ordered
    }

    /// Get the number of call log entries for a route.
    pub fn get_call_log_length(env: Env, route: String) -> u32 {
        env.storage()
            .instance()
            .get::<DataKey, CallLogState>(&DataKey::CallLog(route))
            .map(|log| log.count)
            .unwrap_or(0)
    }

    /// Get an aggregated summary of call log stats for a route.
    pub fn get_call_log_summary(env: Env, route: String) -> Option<CallLogSummary> {
        env.storage()
            .instance()
            .get(&DataKey::CallLogSummary(route))
    }

    /// Clear all call log entries for a route.
    pub fn reset_route_call_log(
        env: Env,
        caller: Address,
        route: String,
    ) -> Result<(), MiddlewareError> {
        caller.require_auth();
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, MiddlewareError)?;

        if let Some(config) = env
            .storage()
            .instance()
            .get::<DataKey, RouteConfig>(&DataKey::RouteConfig(route.clone()))
        {
            if config.log_retention > 0 {
                let placeholder = CallLogEntry {
                    caller: caller.clone(),
                    timestamp: 0,
                    success: false,
                    route: route.clone(),
                };
                let mut entries = Vec::new(&env);
                for _ in 0..config.log_retention {
                    entries.push_back(placeholder.clone());
                }
                let log = CallLogState {
                    entries,
                    head: 0,
                    count: 0,
                };
                env.storage()
                    .instance()
                    .set(&DataKey::CallLog(route.clone()), &log);
            } else {
                env.storage()
                    .instance()
                    .remove(&DataKey::CallLog(route.clone()));
            }
        } else {
            env.storage()
                .instance()
                .remove(&DataKey::CallLog(route.clone()));
        }
        env.events()
            .publish((Symbol::new(&env, "call_log_cleared"),), route);
        Ok(())
    }

    /// Get rate limit state for a caller on a specific route.
    pub fn rate_limit_state(env: Env, route: String, caller: Address) -> Option<RateLimitState> {
        let route_call_state: RouteCallState = env
            .storage()
            .instance()
            .get(&DataKey::RouteCallState(route.clone()))?;
        let state: RateLimitState = route_call_state.rate_limits.get(caller)?;

        if let Some(config) = env
            .storage()
            .instance()
            .get::<DataKey, RouteConfig>(&DataKey::RouteConfig(route))
        {
            let now = env.ledger().timestamp();
            let window_elapsed = now >= state.window_start + config.window_seconds;

            if window_elapsed {
                Some(RateLimitState {
                    calls_in_window: 0,
                    window_start: now,
                    total_violations: state.total_violations,
                })
            } else {
                Some(state)
            }
        } else {
            Some(state)
        }
    }

    /// Get rate limit statistics for a caller on a specific route.
    pub fn get_rate_limit_stats(
        env: Env,
        route: String,
        caller: Address,
    ) -> Option<RateLimitState> {
        Self::rate_limit_state(env, route, caller)
    }

    /// Get aggregated rate limit statistics for a route across all callers.
    pub fn get_route_rate_limit_stats(env: Env, route: String) -> Option<RouteRateLimitStats> {
        let route_call_state: RouteCallState = env
            .storage()
            .instance()
            .get(&DataKey::RouteCallState(route.clone()))?;

        if route_call_state.rate_limits.is_empty() {
            return None;
        }

        let mut total_calls_in_window: u32 = 0;
        let mut total_violations: u32 = 0;
        let mut earliest_window_start: u64 = u64::MAX;

        let config = env
            .storage()
            .instance()
            .get::<DataKey, RouteConfig>(&DataKey::RouteConfig(route.clone()));
        let now = env.ledger().timestamp();

        for (_caller, state) in route_call_state.rate_limits.iter() {
            let (calls, window_start) = if let Some(ref cfg) = config {
                let window_elapsed = now >= state.window_start + cfg.window_seconds;
                if window_elapsed { (0, now) } else { (state.calls_in_window, state.window_start) }
            } else {
                (state.calls_in_window, state.window_start)
            };

            total_calls_in_window += calls;
            total_violations += state.total_violations;
            if window_start < earliest_window_start {
                earliest_window_start = window_start;
            }
        }

        let final_window_start = if earliest_window_start == u64::MAX {
            now
        } else {
            earliest_window_start
        };

        Some(RouteRateLimitStats {
            total_calls_in_window,
            window_start: final_window_start,
            total_violations,
        })
    }

    /// Reset rate limit state for a caller on a specific route.
    pub fn reset_rate_limit(
        env: Env,
        caller: Address,
        route: String,
        target_caller: Address,
    ) -> Result<(), MiddlewareError> {
        caller.require_auth();
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, MiddlewareError)?;

        let mut route_call_state: RouteCallState = env
            .storage()
            .instance()
            .get(&DataKey::RouteCallState(route.clone()))
            .unwrap_or(RouteCallState {
                rate_limits: Map::new(&env),
                circuit_breaker: CircuitBreakerState {
                    failure_count: 0,
                    opened_at: 0,
                    is_open: false,
                    is_half_open: false,
                },
            });

        route_call_state.rate_limits.remove(target_caller.clone());
        env.storage()
            .instance()
            .set(&DataKey::RouteCallState(route), &route_call_state);

        Ok(())
    }

    /// Returns the RouteConfig for a specific route.
    pub fn route_config(env: Env, route: String) -> Option<RouteConfig> {
        env.storage().instance().get(&DataKey::RouteConfig(route))
    }

    /// Returns all route names that have been configured via configure_route.
    pub fn get_configured_routes(env: Env) -> Vec<String> {
        env.storage()
            .instance()
            .get(&DataKey::ConfiguredRoutes)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get the current circuit breaker state for a route.
    pub fn circuit_breaker_state(env: Env, route: String) -> Option<CircuitBreakerState> {
        let route_call_state: RouteCallState = env
            .storage()
            .instance()
            .get(&DataKey::RouteCallState(route))?;
        Some(route_call_state.circuit_breaker)
    }

    /// Get current admin.
    pub fn admin(env: Env) -> Result<Address, MiddlewareError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(MiddlewareError::NotInitialized)
    }

    /// Set the rate limit exceeded response strategy for a route.
    pub fn set_rate_limit_strategy(
        env: Env,
        caller: Address,
        route: String,
        strategy: RateLimitStrategy,
    ) -> Result<(), MiddlewareError> {
        caller.require_auth();
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, MiddlewareError)?;
        env.storage()
            .instance()
            .set(&DataKey::RateLimitStrategy(route.clone()), &strategy);
        env.events().publish(
            (Symbol::new(&env, "rate_limit_strategy_set"),),
            (route, strategy),
        );
        Ok(())
    }

    /// Get the rate limit strategy for a route.
    pub fn get_rate_limit_strategy(env: Env, route: String) -> RateLimitStrategy {
        env.storage()
            .instance()
            .get(&DataKey::RateLimitStrategy(route))
            .unwrap_or(RateLimitStrategy::Reject)
    }

    /// Reset circuit breaker for a route.
    pub fn reset_circuit_breaker(
        env: Env,
        caller: Address,
        route: String,
    ) -> Result<(), MiddlewareError> {
        caller.require_auth();
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, MiddlewareError)?;

        let reset_state = CircuitBreakerState {
            failure_count: 0,
            opened_at: 0,
            is_open: false,
            is_half_open: false,
        };
        let mut route_call_state: RouteCallState = env
            .storage()
            .instance()
            .get(&DataKey::RouteCallState(route.clone()))
            .unwrap_or(RouteCallState {
                rate_limits: Map::new(&env),
                circuit_breaker: CircuitBreakerState {
                    failure_count: 0,
                    opened_at: 0,
                    is_open: false,
                    is_half_open: false,
                },
            });
        route_call_state.circuit_breaker = reset_state;
        env.storage()
            .instance()
            .set(&DataKey::RouteCallState(route), &route_call_state);
        Ok(())
    }

    /// Transfer admin to a new address.
    pub fn transfer_admin(
        env: Env,
        current: Address,
        new_admin: Address,
    ) -> Result<(), MiddlewareError> {
        current.require_auth();
        router_common::require_admin_simple!(&env, &current, &DataKey::Admin, MiddlewareError)?;
        router_common::admin_transfer_complete!(&env, &current, &new_admin, &DataKey::Admin);
        Ok(())
    }

    /// Configure a per-caller rate limit override for a specific route.
    pub fn configure_caller_rate_limit(
        env: Env,
        caller: Address,
        route: String,
        target_caller: Address,
        max_calls: u32,
        window_secs: u64,
    ) -> Result<(), MiddlewareError> {
        caller.require_auth();
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, MiddlewareError)?;

        let key = DataKey::CallerRateLimit(route.clone(), target_caller.clone());
        env.storage().instance().set(
            &key,
            &CallerRateLimitConfig {
                max_calls,
                window_secs,
            },
        );
        env.events().publish(
            (Symbol::new(&env, "caller_rate_limit_set"),),
            (route, target_caller, max_calls, window_secs),
        );
        Ok(())
    }

    /// Set a per-caller rate limit override for a specific route.
    ///
    /// When a `CallerRateLimit` entry is present for `(route, target_caller)`,
    /// `pre_call` uses `max_calls` / `window_secs` instead of the route-level
    /// `max_calls_per_window` / `window_seconds` for that caller only. This
    /// allows privileged callers to receive higher limits and suspicious
    /// addresses to receive tighter limits without touching the global config.
    ///
    /// Admin only. Emits a `caller_rate_limit_set` event.
    pub fn set_caller_rate_limit(
        env: Env,
        caller: Address,
        route: String,
        target_caller: Address,
        max_calls: u32,
        window_secs: u64,
    ) -> Result<(), MiddlewareError> {
        // Delegate to configure_caller_rate_limit for DRY implementation.
        Self::configure_caller_rate_limit(env, caller, route, target_caller, max_calls, window_secs)
    }

    /// Remove a per-caller rate limit override, restoring the route-level default.
    ///
    /// After removal `pre_call` falls back to `RouteConfig::max_calls_per_window`
    /// for the caller. Admin only. Emits a `caller_rate_limit_removed` event.
    pub fn remove_caller_rate_limit(
        env: Env,
        caller: Address,
        route: String,
        target_caller: Address,
    ) -> Result<(), MiddlewareError> {
        caller.require_auth();
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, MiddlewareError)?;

        let key = DataKey::CallerRateLimit(route.clone(), target_caller.clone());
        env.storage().instance().remove(&key);
        env.events().publish(
            (Symbol::new(&env, "caller_rate_limit_removed"),),
            (route, target_caller),
        );
        Ok(())
    }

    /// Get the per-caller rate limit override for a specific route and caller.
    ///
    /// Returns `None` if no override has been set (the route-level default applies).
    pub fn get_caller_rate_limit(
        env: Env,
        route: String,
        target_caller: Address,
    ) -> Option<CallerRateLimitConfig> {
        env.storage()
            .instance()
            .get(&DataKey::CallerRateLimit(route, target_caller))
    }

    /// Check whether a specific caller has exceeded their per-caller rate limit.
    pub fn check_caller_rate_limit(
        env: Env,
        route: String,
        caller: Address,
    ) -> Result<bool, MiddlewareError> {
        let key = DataKey::CallerRateLimit(route.clone(), caller.clone());
        if let Some(config) = env
            .storage()
            .instance()
            .get::<DataKey, CallerRateLimitConfig>(&key)
        {
            let route_call_state: RouteCallState = env
                .storage()
                .instance()
                .get(&DataKey::RouteCallState(route.clone()))
                .unwrap_or(RouteCallState {
                    rate_limits: Map::new(&env),
                    circuit_breaker: CircuitBreakerState {
                        failure_count: 0,
                        opened_at: 0,
                        is_open: false,
                        is_half_open: false,
                    },
                });

            let state: RateLimitState = route_call_state
                .rate_limits
                .get(caller.clone())
                .unwrap_or(RateLimitState {
                    calls_in_window: 0,
                    window_start: env.ledger().timestamp(),
                    total_violations: 0,
                });

            let now = env.ledger().timestamp();
            let window_elapsed = now >= state.window_start + config.window_secs;
            let calls = if window_elapsed { 0 } else { state.calls_in_window };

            Ok(calls < config.max_calls)
        } else {
            Ok(true)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        Env, FromVal, IntoVal, String,
    };

    fn setup() -> (Env, Address, RouterMiddlewareClient<'static>) {
        let env = Env::default();
        env.ledger().set_timestamp(123456);
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterMiddleware);
        let client = RouterMiddlewareClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    #[test]
    fn test_rate_limit_state_not_written_when_route_disabled_before_commit() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        // Enable route with a rate limit of 5 calls per window
        client.configure_route(&admin, &route, &5, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);

        assert!(client.try_pre_call(&caller, &route).is_ok());
        let state_after_first = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state_after_first.calls_in_window, 1);

        // Disable the route
        client.configure_route(&admin, &route, &5, &60, &false, &0, &0, &0, &0);

        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::RouteDisabled))
        );

        let state_after_rejected = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state_after_rejected.calls_in_window, 1);

        assert_eq!(client.total_calls(), 1);

        // Re-enable the route — no stale state should affect the next call
        client.configure_route(&admin, &route, &5, &60, &true, &0, &0, &0, &0);
        assert!(client.try_pre_call(&caller, &route).is_ok());
        let state_after_reenable = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state_after_reenable.calls_in_window, 2);
    }

    #[test]
    fn test_global_disable_does_not_write_rate_limit_state() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &5, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);
        assert_eq!(client.total_calls(), 1);

        client.set_global_enabled(&admin, &false);

        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::MiddlewareDisabled))
        );
        assert_eq!(client.total_calls(), 1);
        let state = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state.calls_in_window, 1);
    }

    #[test]
    fn test_pre_call_no_config_passes() {
        let (env, _, client) = setup();
        let caller = Address::generate(&env);
        let route = String::from_str(&env, "oracle/get_price");
        let result = client.try_pre_call(&caller, &route);
        assert!(result.is_ok());
        assert_eq!(client.total_calls(), 1);
    }

    #[test]
    fn test_rate_limit_enforced() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        // max 2 calls per 60s window
        client.configure_route(&admin, &route, &2, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);
        client.pre_call(&caller, &route);
        let result = client.try_pre_call(&caller, &route);
        assert_eq!(result, Err(Ok(MiddlewareError::RateLimitExceeded)));
    }

    #[test]
    fn test_rate_limit_resets_after_window() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);
        env.ledger().with_mut(|l| l.timestamp += 61);
        let result = client.try_pre_call(&caller, &route);
        assert!(result.is_ok());
    }

    #[test]
    fn test_disabled_route_blocked() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &false, &0, &0, &0, &0);
        let caller = Address::generate(&env);
        let result = client.try_pre_call(&caller, &route);
        assert_eq!(result, Err(Ok(MiddlewareError::RouteDisabled)));
    }

    #[test]
    fn test_global_disable_blocks_all() {
        let (env, admin, client) = setup();
        client.set_global_enabled(&admin, &false);
        let caller = Address::generate(&env);
        let route = String::from_str(&env, "any/route");
        let result = client.try_pre_call(&caller, &route);
        assert_eq!(result, Err(Ok(MiddlewareError::MiddlewareDisabled)));
    }

    #[test]
    fn test_unauthorized_configure_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let route = String::from_str(&env, "oracle/get_price");
        let result = client.try_configure_route(&attacker, &route, &10, &60, &true, &0, &0, &0, &0);
        assert_eq!(result, Err(Ok(MiddlewareError::Unauthorized)));
    }

    #[test]
    fn test_post_call_succeeds() {
        let (env, _, client) = setup();
        let caller = Address::generate(&env);
        let route = String::from_str(&env, "oracle/get_price");

        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);
    }

    #[test]
    fn test_get_call_log_length_zero_before_calls() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &3, &0);

        assert_eq!(client.get_call_log_length(&route), 0);
    }

    #[test]
    fn test_get_call_log_length_matches_get_call_log() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &5, &0);

        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);

        assert_eq!(
            client.get_call_log_length(&route),
            client.get_call_log(&route).len()
        );
    }

    #[test]
    fn test_get_call_log_length_respects_retention() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &2, &0);

        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &true);

        assert_eq!(client.get_call_log_length(&route), 2);
        assert_eq!(client.get_call_log(&route).len(), 2);
    }

    #[test]
    fn test_rate_limit_isolated_per_route() {
        let (env, admin, client) = setup();
        let route_a = String::from_str(&env, "oracle/price");
        let route_b = String::from_str(&env, "vault/deposit");
        // route_a: 10 calls per minute, route_b: 5 calls per minute
        client.configure_route(&admin, &route_a, &10, &60, &true, &0, &0, &0, &0);
        client.configure_route(&admin, &route_b, &5, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        for _ in 0..4 {
            client.pre_call(&caller, &route_a);
        }
        assert!(client.try_pre_call(&caller, &route_b).is_ok());
        for _ in 0..4 {
            client.pre_call(&caller, &route_b);
        }
        assert_eq!(
            client.try_pre_call(&caller, &route_b),
            Err(Ok(MiddlewareError::RateLimitExceeded))
        );
        assert!(client.try_pre_call(&caller, &route_a).is_ok());
    }

    #[test]
    fn test_total_calls_not_incremented_on_rejected_pre_call() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);
        assert_eq!(client.total_calls(), 1);

        let _ = client.try_pre_call(&caller, &route);
        assert_eq!(client.total_calls(), 1);
    }

    #[test]
    fn test_admin_getter() {
        let (_env, admin, client) = setup();
        let retrieved_admin = client.admin().unwrap();
        assert_eq!(retrieved_admin, admin);
    }

    #[test]
    fn test_transfer_admin() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin).unwrap();
        assert_eq!(client.admin().unwrap(), new_admin);
    }

    #[test]
    fn test_unauthorized_transfer_admin_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let result = client.try_transfer_admin(&attacker, &new_admin);
        assert_eq!(result, Err(Ok(MiddlewareError::Unauthorized)));
    }

    #[test]
    fn test_circuit_breaker_blocks_calls() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        // Configure route with failure_threshold = 1, no recovery window for simplicity
        client.configure_route(&admin, &route, &0, &0, &true, &1, &0, &0, &0);

        let caller = Address::generate(&env);
        assert!(client.try_pre_call(&caller, &route).is_ok());
        client.post_call(&caller, &route, &false);
        let result = client.try_pre_call(&caller, &route);
        assert_eq!(result, Err(Ok(MiddlewareError::CircuitOpen)));
    }

    #[test]
    fn test_reset_circuit_breaker() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &1, &0, &0, &0);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);
        client.post_call(&caller, &route, &false);
        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );

        client.reset_circuit_breaker(&admin, &route).unwrap();
        assert!(client.try_pre_call(&caller, &route).is_ok());
    }

    #[test]
    fn test_circuit_breaker_unauthorized_reset() {
        let (env, _admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let attacker = Address::generate(&env);
        let result = client.try_reset_circuit_breaker(&attacker, &route);
        assert_eq!(result, Err(Ok(MiddlewareError::Unauthorized)));
    }

    #[test]
    fn test_transfer_admin_emits_event() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, RouterMiddleware);
        let client = RouterMiddlewareClient::new(&env, &contract_id);

        let old_admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        client.initialize(&old_admin).unwrap();
        client.transfer_admin(&old_admin, &new_admin).unwrap();

        let events = env.events().all();
        let last_event = events.last().unwrap();

        assert_eq!(last_event.0, contract_id);

        let topic: Symbol = last_event.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, "admin_transferred"));

        let (emitted_old, emitted_new): (Address, Address) = last_event.2.into_val(&env);
        assert_eq!(emitted_old, old_admin);
        assert_eq!(emitted_new, new_admin);
    }

    #[test]
    fn test_success_resets_failure_count() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        // threshold=3, so 2 failures then a success then 2 more should NOT trip
        client.configure_route(&admin, &route, &0, &0, &true, &3, &0, &0, &0);
        let caller = Address::generate(&env);

        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &false);

        let result = client.try_pre_call(&caller, &route);
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_circuit_not_reset_by_success() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &1, &0, &0, &0);
        let caller = Address::generate(&env);

        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &true);

        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );
    }

    #[test]
    fn test_get_configured_routes_empty() {
        let (_env, _admin, client) = setup();
        let routes = client.get_configured_routes();
        assert!(routes.is_empty());
    }

    #[test]
    fn test_get_configured_routes_multiple() {
        let (env, admin, client) = setup();
        let route_a = String::from_str(&env, "oracle/price");
        let route_b = String::from_str(&env, "vault/deposit");
        client.configure_route(&admin, &route_a, &0, &0, &true, &0, &0, &0, &0);
        client.configure_route(&admin, &route_b, &0, &0, &true, &0, &0, &0, &0);
        let routes = client.get_configured_routes();
        assert_eq!(routes.len(), 2);
        assert!(routes.contains(&route_a));
        assert!(routes.contains(&route_b));
    }

    #[test]
    fn test_get_configured_routes_no_duplicates() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/price");
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &0, &0);
        client.configure_route(&admin, &route, &5, &60, &true, &0, &0, &0, &0);
        let routes = client.get_configured_routes();
        assert_eq!(routes.len(), 1);
    }

    #[test]
    fn test_circuit_breaker_state_none_before_failures() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &3, &0, &0, &0);
        assert_eq!(client.circuit_breaker_state(&route), None);
    }

    #[test]
    fn test_circuit_breaker_state_reflects_failures() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &3, &0, &0, &0);
        let caller = Address::generate(&env);
        client.post_call(&caller, &route, &false);
        let state = client.circuit_breaker_state(&route).unwrap();
        assert_eq!(state.failure_count, 1);
        assert!(!state.is_open);
    }

    #[test]
    fn test_circuit_breaker_state_open_after_threshold() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &2, &0, &0, &0);
        let caller = Address::generate(&env);
        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &false);
        let state = client.circuit_breaker_state(&route).unwrap();
        assert!(state.is_open);
        assert!(state.opened_at > 0);
    }

    #[test]
    fn test_circuit_breaker_state_clears_after_reset() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &1, &0, &0, &0);
        let caller = Address::generate(&env);
        client.post_call(&caller, &route, &false);
        assert!(client.circuit_breaker_state(&route).unwrap().is_open);
        client.reset_circuit_breaker(&admin, &route).unwrap();
        let state = client.circuit_breaker_state(&route).unwrap();
        assert!(!state.is_open);
    }

    #[test]
    fn test_call_log_never_exceeds_retention() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &3, &0);

        let caller = Address::generate(&env);
        for _ in 0..10 {
            client.pre_call(&caller, &route);
            client.post_call(&caller, &route, &true);
        }

        assert_eq!(client.get_call_log(&route).len(), 3);
    }

    #[test]
    fn test_call_log_retains_most_recent() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &3, &0);

        let caller = Address::generate(&env);
        for i in 0..5u64 {
            env.ledger().set_timestamp(1000 + i);
            client.pre_call(&caller, &route);
            client.post_call(&caller, &route, &true);
        }

        let log = client.get_call_log(&route);
        assert_eq!(log.len(), 3);
        assert_eq!(log.get(0).unwrap().timestamp, 1002);
        assert_eq!(log.get(2).unwrap().timestamp, 1004);
    }

    #[test]
    fn test_rate_limit_state_resets_after_window() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        // Configure route with 60 second window and max 5 calls
        client.configure_route(&admin, &route, &5, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);

        for _ in 0..3 {
            client.pre_call(&caller, &route);
        }

        let state_within_window = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state_within_window.calls_in_window, 3);

        env.ledger().set_timestamp(env.ledger().timestamp() + 61);

        let state_after_window = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state_after_window.calls_in_window, 0);
        assert_eq!(
            state_after_window.window_start,
            env.ledger().timestamp()
        );
    }

    #[test]
    fn test_rate_limit_state_within_window_accurate() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        // Configure route with 60 second window
        client.configure_route(&admin, &route, &5, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);

        client.pre_call(&caller, &route);
        client.pre_call(&caller, &route);

        let state = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state.calls_in_window, 2);

        env.ledger().set_timestamp(env.ledger().timestamp() + 30);

        let state_still_in_window = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state_still_in_window.calls_in_window, 2);
        assert_eq!(
            state_still_in_window.window_start, state.window_start,
            "window_start should not change within window"
        );
    }

    #[test]
    fn test_circuit_breaker_auto_recovers_after_window() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        // failure_threshold=1, recovery_window=60s
        client.configure_route(&admin, &route, &0, &0, &true, &1, &60, &0, &0);

        let caller = Address::generate(&env);

        client.post_call(&caller, &route, &false);
        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );

        env.ledger().with_mut(|l| l.timestamp += 61);

        assert!(client.try_pre_call(&caller, &route).is_ok());
    }

    #[test]
    fn test_circuit_not_recovered_before_window_elapses() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &1, &60, &0, &0);

        let caller = Address::generate(&env);
        client.post_call(&caller, &route, &false);

        env.ledger().with_mut(|l| l.timestamp += 30);
        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );
    }

    #[test]
    fn test_circuit_breaker_state_reset_after_recovery() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        // failure_threshold=1, recovery_window=60s
        client.configure_route(&admin, &route, &0, &0, &true, &1, &60, &0, &0);

        let caller = Address::generate(&env);

        client.post_call(&caller, &route, &false);

        let state_when_open = client.circuit_breaker_state(&route).unwrap();
        assert!(state_when_open.is_open);
        assert_eq!(state_when_open.failure_count, 1);

        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );

        env.ledger().with_mut(|l| l.timestamp += 61);

        assert!(client.try_pre_call(&caller, &route).is_ok());
        client.post_call(&caller, &route, &true);

        let state_after_recovery = client.circuit_breaker_state(&route).unwrap();
        assert!(!state_after_recovery.is_open);
        assert_eq!(state_after_recovery.failure_count, 0);
        assert_eq!(state_after_recovery.opened_at, 0);
    }

    #[test]
    fn test_rate_limit_call_at_exact_window_boundary_resets() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/price");
        // max 1 call per 60s window
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        let t0 = env.ledger().timestamp();

        client.pre_call(&caller, &route);
        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::RateLimitExceeded))
        );

        env.ledger().with_mut(|l| l.timestamp = t0 + 60);

        assert!(client.try_pre_call(&caller, &route).is_ok());
    }

    #[test]
    fn test_rate_limit_window_jump_multiple_windows() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/price");
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        let t0 = env.ledger().timestamp();

        client.pre_call(&caller, &route);

        env.ledger().with_mut(|l| l.timestamp = t0 + 300);

        assert!(client.try_pre_call(&caller, &route).is_ok());
        let state = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state.calls_in_window, 1);
        assert_eq!(state.window_start, t0 + 300);
    }

    #[test]
    fn test_configure_route_window_zero_max_zero_is_unlimited() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/price");
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        for _ in 0..20 {
            assert!(client.try_pre_call(&caller, &route).is_ok());
        }
    }

    #[test]
    fn test_set_global_enabled_emits_event() {
        let (env, admin, client) = setup();
        client.set_global_enabled(&admin, &false).unwrap();
        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(
            topic,
            Symbol::new(&env, router_common::EVENT_MIDDLEWARE_ENABLED)
        );
        let emitted: bool = last.2.into_val(&env);
        assert!(!emitted);
    }

    #[test]
    fn test_get_call_log_filtered_empty_when_no_calls() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &10, &0);

        assert!(client.get_call_log_filtered(&route, &true).is_empty());
        assert!(client.get_call_log_filtered(&route, &false).is_empty());
    }

    #[test]
    fn test_get_call_log_filtered_success_only() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &10, &0);

        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &true);

        let filtered = client.get_call_log_filtered(&route, &true);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.get(0).unwrap().success);
        assert!(filtered.get(1).unwrap().success);
    }

    #[test]
    fn test_get_call_log_filtered_failure_only() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &10, &0);

        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &false);

        let filtered = client.get_call_log_filtered(&route, &false);
        assert_eq!(filtered.len(), 2);
        assert!(!filtered.get(0).unwrap().success);
        assert!(!filtered.get(1).unwrap().success);
    }

    #[test]
    fn test_get_call_log_filtered_all_success_no_failures() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &5, &0);

        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &true);

        assert!(client.get_call_log_filtered(&route, &false).is_empty());
        assert_eq!(client.get_call_log_filtered(&route, &true).len(), 2);
    }

    #[test]
    fn test_get_call_log_filtered_with_ring_buffer_wraparound() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);
        // retention=3, make 5 calls so ring buffer wraps
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &3, &0);

        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &true);

        let success = client.get_call_log_filtered(&route, &true);
        let failure = client.get_call_log_filtered(&route, &false);
        assert_eq!(success.len(), 2);
        assert_eq!(failure.len(), 1);
    }

    #[test]
    fn test_get_call_log_summary_none_before_calls() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &5, &0);
        assert_eq!(client.get_call_log_summary(&route), None);
    }

    #[test]
    fn test_get_call_log_summary_counts_correctly() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &10, &0);

        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);

        let summary = client.get_call_log_summary(&route).unwrap();
        assert_eq!(summary.total_calls, 3);
        assert_eq!(summary.success_count, 2);
        assert_eq!(summary.failure_count, 1);
    }

    #[test]
    fn test_get_call_log_summary_last_call_timestamp() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &10, &0);

        env.ledger().set_timestamp(1000);
        client.post_call(&caller, &route, &true);
        env.ledger().set_timestamp(2000);
        client.post_call(&caller, &route, &false);

        let summary = client.get_call_log_summary(&route).unwrap();
        assert_eq!(summary.last_call_timestamp, 2000);
    }

    #[test]
    fn test_get_call_log_summary_not_affected_by_retention_limit() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &2, &0); // retain only 2

        for _ in 0..5 {
            client.post_call(&caller, &route, &true);
        }

        let summary = client.get_call_log_summary(&route).unwrap();
        assert_eq!(summary.total_calls, 5);
        assert_eq!(client.get_call_log(&route).len(), 2);
    }

    #[test]
    fn test_circuit_opens_after_failure_threshold() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);

        // Configure with failure_threshold=3
        client.configure_route(&admin, &route, &0, &0, &true, &3, &60, &0, &0);

        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &false);

        let result = client.try_pre_call(&caller, &route);
        assert_eq!(result, Err(Ok(MiddlewareError::CircuitOpen)));
    }

    #[test]
    fn test_pre_call_blocked_while_circuit_open() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);

        // Configure with failure_threshold=1, recovery_window=60s
        client.configure_route(&admin, &route, &0, &0, &true, &1, &60, &0, &0);

        client.post_call(&caller, &route, &false);

        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );

        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );
        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );
    }

    #[test]
    fn test_pre_call_succeeds_after_recovery_window() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);

        // Configure with failure_threshold=2, recovery_window=100s
        client.configure_route(&admin, &route, &0, &0, &true, &2, &100, &0, &0);

        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &false);

        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );

        env.ledger().with_mut(|l| l.timestamp += 101);

        let result = client.try_pre_call(&caller, &route);
        assert!(
            result.is_ok(),
            "pre_call should succeed after recovery window"
        );
    }

    #[test]
    fn test_success_after_recovery_resets_failure_count() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);

        // Configure with failure_threshold=2, recovery_window=60s
        client.configure_route(&admin, &route, &0, &0, &true, &2, &60, &0, &0);

        client.post_call(&caller, &route, &false);
        client.post_call(&caller, &route, &false);

        let state_before_recovery = client.circuit_breaker_state(&route).unwrap();
        assert!(state_before_recovery.is_open);
        assert_eq!(state_before_recovery.failure_count, 2);

        env.ledger().with_mut(|l| l.timestamp += 61);

        assert!(client.try_pre_call(&caller, &route).is_ok());
        client.post_call(&caller, &route, &true);

        let state_after_recovery = client.circuit_breaker_state(&route).unwrap();
        assert!(!state_after_recovery.is_open);
        assert_eq!(state_after_recovery.failure_count, 0);
        assert_eq!(state_after_recovery.opened_at, 0);
    }

    #[test]
    fn test_circuit_breaker_state_updated_after_recovery() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);

        // Configure with failure_threshold=1, recovery_window=50s
        client.configure_route(&admin, &route, &0, &0, &true, &1, &50, &0, &0);

        client.post_call(&caller, &route, &false);

        let state_open = client.circuit_breaker_state(&route).unwrap();
        assert!(state_open.is_open);
        assert_eq!(state_open.failure_count, 1);
        assert!(state_open.opened_at > 0);

        env.ledger().with_mut(|l| l.timestamp += 51);

        assert!(client.try_pre_call(&caller, &route).is_ok());
        client.post_call(&caller, &route, &true);

        let state_recovered = client.circuit_breaker_state(&route).unwrap();
        assert!(!state_recovered.is_open, "is_open should be false");
        assert_eq!(
            state_recovered.failure_count, 0,
            "failure_count should be 0"
        );
        assert_eq!(state_recovered.opened_at, 0, "opened_at should be 0");
    }

    #[test]
    fn test_circuit_closed_event_emitted_on_recovery() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);

        // failure_threshold=1, recovery_window=60s
        client.configure_route(&admin, &route, &0, &0, &true, &1, &60, &0, &0);

        client.post_call(&caller, &route, &false);
        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );

        env.ledger().with_mut(|l| l.timestamp += 61);

        assert!(client.try_pre_call(&caller, &route).is_ok());

        client.post_call(&caller, &route, &true);

        let events = env.events().all();
        let closed_event = events.iter().find(|e| {
            e.1.get(0)
                .map(|v| {
                    let s: Symbol = v.into_val(&env);
                    s == Symbol::new(&env, router_common::EVENT_CIRCUIT_CLOSED)
                })
                .unwrap_or(false)
        });
        assert!(
            closed_event.is_some(),
            "circuit_closed event must be emitted"
        );

        let state = client.circuit_breaker_state(&route).unwrap();
        assert!(!state.is_open);
        assert!(!state.is_half_open);
        assert_eq!(state.failure_count, 0);
    }

    #[test]
    fn test_circuit_not_recovered_before_window_expires() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);

        // Configure with failure_threshold=1, recovery_window=100s
        client.configure_route(&admin, &route, &0, &0, &true, &1, &100, &0, &0);

        // Trip the circuit
        client.post_call(&caller, &route, &false);

        // Advance time but not enough (only 50 seconds, need 100)
        env.ledger().with_mut(|l| l.timestamp += 50);

        // Circuit should still be open
        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::CircuitOpen))
        );

        // Advance to exactly the recovery time (not past it)
        env.ledger().with_mut(|l| l.timestamp += 50);

        // Should now succeed (at exactly recovery_window_seconds)
        assert!(client.try_pre_call(&caller, &route).is_ok());
    }

    // ── Issue #577: RateLimitStrategy (Reject/Throttle/LogOnly) ───────────────

    #[test]
    fn test_rate_limit_strategy_default_is_reject() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);
        let result = client.try_pre_call(&caller, &route);
        assert_eq!(result, Err(Ok(MiddlewareError::RateLimitExceeded)));
    }

    #[test]
    fn test_set_and_get_rate_limit_strategy() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");

        assert_eq!(
            client.get_rate_limit_strategy(&route),
            RateLimitStrategy::Reject
        );

        client.set_rate_limit_strategy(&admin, &route, &RateLimitStrategy::Throttle).unwrap();
        assert_eq!(
            client.get_rate_limit_strategy(&route),
            RateLimitStrategy::Throttle
        );

        client.set_rate_limit_strategy(&admin, &route, &RateLimitStrategy::LogOnly).unwrap();
        assert_eq!(
            client.get_rate_limit_strategy(&route),
            RateLimitStrategy::LogOnly
        );

        client.set_rate_limit_strategy(&admin, &route, &RateLimitStrategy::Reject).unwrap();
        assert_eq!(
            client.get_rate_limit_strategy(&route),
            RateLimitStrategy::Reject
        );
    }

    #[test]
    fn test_throttle_strategy_allows_call_after_rate_limit_exceeded() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        // max 1 call per 60s window
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);
        client.set_rate_limit_strategy(&admin, &route, &RateLimitStrategy::Throttle);

        let caller = Address::generate(&env);
        assert!(client.try_pre_call(&caller, &route).is_ok());
        assert!(client.try_pre_call(&caller, &route).is_ok());
        assert_eq!(client.total_calls(), 2);
    }

    #[test]
    fn test_throttle_strategy_emits_event() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);
        client.set_rate_limit_strategy(&admin, &route, &RateLimitStrategy::Throttle);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);
        client.pre_call(&caller, &route);

        let events = env.events().all();
        let throttled_event = events.iter().find(|(_, topics, _)| {
            topics
                .get(0)
                .map(|v| {
                    Symbol::from_val(&env, &v)
                        == Symbol::new(&env, router_common::EVENT_RATE_LIMIT_THROTTLED)
                })
                .unwrap_or(false)
        });
        assert!(
            throttled_event.is_some(),
            "rate_limit_throttled event should be emitted"
        );
    }

    #[test]
    fn test_log_only_strategy_allows_call_after_rate_limit_exceeded() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        // max 1 call per 60s window
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);
        client.set_rate_limit_strategy(&admin, &route, &RateLimitStrategy::LogOnly);

        let caller = Address::generate(&env);
        assert!(client.try_pre_call(&caller, &route).is_ok());
        assert!(client.try_pre_call(&caller, &route).is_ok());
        assert!(client.try_pre_call(&caller, &route).is_ok());
        assert_eq!(client.total_calls(), 3);
    }

    #[test]
    fn test_log_only_strategy_emits_event() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);
        client.set_rate_limit_strategy(&admin, &route, &RateLimitStrategy::LogOnly);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);
        client.pre_call(&caller, &route);

        let events = env.events().all();
        let log_event = events.iter().find(|(_, topics, _)| {
            topics
                .get(0)
                .map(|v| {
                    Symbol::from_val(&env, &v)
                        == Symbol::new(&env, router_common::EVENT_RATE_LIMIT_EXCEEDED)
                })
                .unwrap_or(false)
        });
        assert!(
            log_event.is_some(),
            "rate_limit_exceeded event should be emitted"
        );
    }

    #[test]
    fn test_rate_limit_strategy_is_per_route() {
        let (env, admin, client) = setup();
        let route_a = String::from_str(&env, "oracle/price");
        let route_b = String::from_str(&env, "vault/deposit");

        // Both routes have max 1 call per window
        client.configure_route(&admin, &route_a, &1, &60, &true, &0, &0, &0, &0);
        client.configure_route(&admin, &route_b, &1, &60, &true, &0, &0, &0, &0);

        client.set_rate_limit_strategy(&admin, &route_a, &RateLimitStrategy::Throttle).unwrap();

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route_a);
        client.pre_call(&caller, &route_b);

        assert!(client.try_pre_call(&caller, &route_a).is_ok());
        assert_eq!(
            client.try_pre_call(&caller, &route_b),
            Err(Ok(MiddlewareError::RateLimitExceeded))
        );
    }

    #[test]
    fn test_set_rate_limit_strategy_unauthorized_fails() {
        let (env, _admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let attacker = Address::generate(&env);
        let result =
            client.try_set_rate_limit_strategy(&attacker, &route, &RateLimitStrategy::Throttle);
        assert_eq!(result, Err(Ok(MiddlewareError::Unauthorized)));
    }

    #[test]
    fn test_set_rate_limit_strategy_emits_event() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");

        client.set_rate_limit_strategy(&admin, &route, &RateLimitStrategy::Throttle).unwrap();

        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, "rate_limit_strategy_set"));
    }

    #[test]
    fn test_rate_limit_large_timestamp_gap_resets_window() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        env.ledger().set_timestamp(100);
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);
        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::RateLimitExceeded))
        );

        env.ledger().set_timestamp(10000);

        assert!(client.try_pre_call(&caller, &route).is_ok());
        let state = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state.calls_in_window, 1);
        assert_eq!(state.window_start, 10000);
    }

    #[test]
    fn test_rate_limit_at_exact_window_boundary() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let t0 = env.ledger().timestamp();
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);
        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::RateLimitExceeded))
        );

        env.ledger().with_mut(|l| l.timestamp = t0 + 60);

        assert!(client.try_pre_call(&caller, &route).is_ok());
        let state = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state.calls_in_window, 1);
        assert_eq!(state.window_start, t0 + 60);
    }

    #[test]
    fn test_rate_limit_multiple_calls_same_ledger_timestamp() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &3, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        let ts = env.ledger().timestamp();

        client.pre_call(&caller, &route);
        client.pre_call(&caller, &route);
        client.pre_call(&caller, &route);

        assert_eq!(
            client.try_pre_call(&caller, &route),
            Err(Ok(MiddlewareError::RateLimitExceeded))
        );

        let state = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state.calls_in_window, 3);
        assert_eq!(state.window_start, ts);
    }

    #[test]
    fn test_rate_limit_window_reset_race_at_boundary() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let t0 = env.ledger().timestamp();
        client.configure_route(&admin, &route, &1, &60, &true, &0, &0, &0, &0);

        let caller_a = Address::generate(&env);
        let caller_b = Address::generate(&env);

        client.pre_call(&caller_a, &route);
        client.pre_call(&caller_b, &route);

        env.ledger().with_mut(|l| l.timestamp = t0 + 60);

        assert!(client.try_pre_call(&caller_a, &route).is_ok());
        assert!(client.try_pre_call(&caller_b, &route).is_ok());

        let state_a = client.rate_limit_state(&route, &caller_a).unwrap();
        let state_b = client.rate_limit_state(&route, &caller_b).unwrap();
        assert_eq!(state_a.calls_in_window, 1);
        assert_eq!(state_b.calls_in_window, 1);
    }

    #[test]
    fn test_rate_limit_no_underflow_on_backward_timestamp() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        env.ledger().set_timestamp(10000);
        client.configure_route(&admin, &route, &5, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        client.pre_call(&caller, &route);

        env.ledger().set_timestamp(9000);

        assert!(client.try_pre_call(&caller, &route).is_ok());
        let state = client.rate_limit_state(&route, &caller).unwrap();
        assert_eq!(state.calls_in_window, 2);
        assert_eq!(state.window_start, 10000);
    }

    // ── Issue #634: get_call_log modulo by zero panic ─────────────────────────

    #[test]
    fn test_get_call_log_with_log_retention_zero_returns_empty() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);

        // Configure with retention=5, make some calls
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &5, &0);
        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);

        // Reconfigure with retention=0 (logging disabled)
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &0, &0);

        // get_call_log must not panic; should return empty
        let log = client.get_call_log(&route);
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn test_get_call_log_filtered_with_log_retention_zero_returns_empty() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        let caller = Address::generate(&env);

        // Configure with retention=5, make some calls
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &5, &0);
        client.post_call(&caller, &route, &true);
        client.post_call(&caller, &route, &false);

        // Reconfigure with retention=0 (logging disabled)
        client.configure_route(&admin, &route, &0, &0, &true, &0, &0, &0, &0);

        // get_call_log_filtered must not panic; should return empty
        let log = client.get_call_log_filtered(&route, &true);
        assert_eq!(log.len(), 0);
        let log = client.get_call_log_filtered(&route, &false);
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn test_burst_allowance_permits_extra_calls() {
        let (env, admin, client) = setup();
        let caller = Address::generate(&env);
        let route = String::from_str(&env, "oracle/get_price");

        // max=2, burst=1 → 3 calls allowed before rejection
        client.configure_route(&admin, &route, &2, &60, &true, &0, &0, &0, &1);

        client.pre_call(&caller, &route); // call 1
        client.pre_call(&caller, &route); // call 2 (at max)
        client.pre_call(&caller, &route); // call 3 (burst)
        let result = client.try_pre_call(&caller, &route); // call 4 → rejected
        assert_eq!(result, Err(Ok(MiddlewareError::RateLimitExceeded)));
    }

    // ── Per-caller rate limit override tests ────────────────────────────────

    #[test]
    fn test_caller_override_higher_limit_allows_more_calls() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");

        // Global limit: 2 calls per 60 s
        client.configure_route(&admin, &route, &2, &60, &true, &0, &0, &0, &0);

        let privileged = Address::generate(&env);

        // Grant privileged caller 10 calls per window
        client.set_caller_rate_limit(&admin, &route, &privileged, &10, &60);

        // Verify override is stored
        let cfg = client.get_caller_rate_limit(&route, &privileged).unwrap();
        assert_eq!(cfg.max_calls, 10);
        assert_eq!(cfg.window_secs, 60);

        // Privileged caller can make 5 calls — well above the global limit of 2
        for _ in 0..5 {
            assert!(client.try_pre_call(&privileged, &route).is_ok());
        }

        // A normal caller is still capped at 2
        let normal = Address::generate(&env);
        assert!(client.try_pre_call(&normal, &route).is_ok());
        assert!(client.try_pre_call(&normal, &route).is_ok());
        assert_eq!(
            client.try_pre_call(&normal, &route),
            Err(Ok(MiddlewareError::RateLimitExceeded))
        );
    }

    #[test]
    fn test_caller_override_lower_limit_throttles_caller() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");

        // Global limit: 10 calls per 60 s
        client.configure_route(&admin, &route, &10, &60, &true, &0, &0, &0, &0);

        let restricted = Address::generate(&env);

        // Restrict suspicious caller to 1 call per window
        client.set_caller_rate_limit(&admin, &route, &restricted, &1, &60);

        // First call succeeds
        assert!(client.try_pre_call(&restricted, &route).is_ok());

        // Second call must be rejected even though global limit is 10
        assert_eq!(
            client.try_pre_call(&restricted, &route),
            Err(Ok(MiddlewareError::RateLimitExceeded))
        );

        // Other callers are unaffected — still get 10 calls
        let other = Address::generate(&env);
        for _ in 0..5 {
            assert!(client.try_pre_call(&other, &route).is_ok());
        }
    }

    #[test]
    fn test_remove_caller_rate_limit_restores_default() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");

        // Global limit: 2 calls per 60 s
        client.configure_route(&admin, &route, &2, &60, &true, &0, &0, &0, &0);

        let privileged = Address::generate(&env);

        // Grant 10-call override, then remove it
        client.set_caller_rate_limit(&admin, &route, &privileged, &10, &60);
        client.remove_caller_rate_limit(&admin, &route, &privileged);

        // Override must be gone
        assert!(client.get_caller_rate_limit(&route, &privileged).is_none());

        // Caller is now subject to the global limit of 2
        assert!(client.try_pre_call(&privileged, &route).is_ok());
        assert!(client.try_pre_call(&privileged, &route).is_ok());
        assert_eq!(
            client.try_pre_call(&privileged, &route),
            Err(Ok(MiddlewareError::RateLimitExceeded))
        );
    }

    #[test]
    fn test_set_caller_rate_limit_emits_event() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &5, &60, &true, &0, &0, &0, &0);

        let target = Address::generate(&env);
        client.set_caller_rate_limit(&admin, &route, &target, &20, &120);

        let events = env.events().all();
        let topic = Symbol::new(&env, "caller_rate_limit_set");
        let found = events.iter().any(|e| {
            e.1.get(0)
                .map(|t| Symbol::from_val(&env, &t) == topic)
                .unwrap_or(false)
        });
        assert!(found, "caller_rate_limit_set event must be emitted");
    }

    #[test]
    fn test_remove_caller_rate_limit_emits_event() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &5, &60, &true, &0, &0, &0, &0);

        let target = Address::generate(&env);
        client.set_caller_rate_limit(&admin, &route, &target, &20, &120);
        client.remove_caller_rate_limit(&admin, &route, &target);

        let events = env.events().all();
        let topic = Symbol::new(&env, "caller_rate_limit_removed");
        let found = events.iter().any(|e| {
            e.1.get(0)
                .map(|t| Symbol::from_val(&env, &t) == topic)
                .unwrap_or(false)
        });
        assert!(found, "caller_rate_limit_removed event must be emitted");
    }

    #[test]
    fn test_get_caller_rate_limit_returns_none_when_not_set() {
        let (env, admin, client) = setup();
        let route = String::from_str(&env, "oracle/get_price");
        client.configure_route(&admin, &route, &5, &60, &true, &0, &0, &0, &0);

        let caller = Address::generate(&env);
        assert!(
            client.get_caller_rate_limit(&route, &caller).is_none(),
            "no override should return None"
        );
    }
}
