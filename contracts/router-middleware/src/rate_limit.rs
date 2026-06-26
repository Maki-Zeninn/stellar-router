//! Per-caller rate limiting logic for router-middleware.
//!
//! Implements a fixed-window counter keyed by `(route, caller)`. The window
//! resets on the first call after `window_seconds` has elapsed since
//! `window_start`. Logic is called from [`pre_call`] in `lib.rs`.

use soroban_sdk::{Address, Env, String};

use crate::{DataKey, RateLimitState, RouteCallState, RouteConfig};

/// Check and update the rate limit for `caller` on `route`.
///
/// Returns `true` if the rate limit is exceeded (call should be blocked).
pub fn check_and_increment(
    env: &Env,
    caller: &Address,
    route: &String,
    config: &RouteConfig,
    route_call_state: &mut RouteCallState,
) -> bool {
    if config.max_calls_per_window == 0 {
        return false; // unlimited
    }

    let now = env.ledger().timestamp();
    let state: RateLimitState =
        route_call_state
            .rate_limits
            .get(caller.clone())
            .unwrap_or(RateLimitState {
                calls_in_window: 0,
                window_start: now,
                total_violations: 0,
            });

    let window_elapsed = now >= state.window_start + config.window_seconds;
    let calls = if window_elapsed {
        0
    } else {
        state.calls_in_window
    };
    let window_start = if window_elapsed {
        now
    } else {
        state.window_start
    };

    if calls >= config.max_calls_per_window {
        // Record violation without incrementing the call count
        let mut violation_state = state.clone();
        violation_state.total_violations = violation_state.total_violations.saturating_add(1);
        route_call_state
            .rate_limits
            .set(caller.clone(), violation_state);
        env.storage()
            .instance()
            .set(&DataKey::RouteCallState(route.clone()), route_call_state);
        return true;
    }

    route_call_state.rate_limits.set(
        caller.clone(),
        RateLimitState {
            calls_in_window: calls + 1,
            window_start,
            total_violations: state.total_violations,
        },
    );
    false
}

/// Reset the rate limit state for a specific caller on a route.
pub fn reset_for_caller(env: &Env, route: &String, caller: &Address) {
    if let Some(mut state) = env
        .storage()
        .instance()
        .get::<DataKey, RouteCallState>(&DataKey::RouteCallState(route.clone()))
    {
        state.rate_limits.remove(caller.clone());
        env.storage()
            .instance()
            .set(&DataKey::RouteCallState(route.clone()), &state);
    }
}
