//! Per-caller rate limiting logic for router-middleware.
//!
//! Implements a fixed-window counter keyed by `(route, caller)`. The window
//! resets on the first call after `effective_window` seconds has elapsed
//! since `window_start`. The limit/window a caller is actually checked
//! against may come from the route-level default (`RouteConfig` plus
//! `burst_allowance`) or from a per-caller `CallerRateLimitConfig` override —
//! resolving which one applies is the caller's job (see [`crate::pre_call`]);
//! this module only owns the window/counter arithmetic so it isn't
//! duplicated between the route-level and per-caller-override paths.

use soroban_sdk::{Address, Env, String};

use crate::{DataKey, RateLimitState, RouteCallState};

/// Outcome of checking a caller's rate limit state against an effective
/// limit/window.
pub struct RateLimitCheck {
    /// `true` if the caller has exceeded `effective_limit` for the current window.
    pub exceeded: bool,
    /// The `RateLimitState` that should be stored for `caller` regardless of
    /// whether the limit was exceeded (callers should still persist this to
    /// track the rolling window / violation count).
    pub updated_state: RateLimitState,
}

/// Check and compute the updated rate limit state for `caller` on a route,
/// given the already-resolved `effective_limit` / `effective_window` (which
/// may come from the route default or a per-caller override).
///
/// This function does not mutate or persist any storage itself — the caller
/// is expected to apply `updated_state` to `route_call_state.rate_limits`
/// and decide how to react to `exceeded` (reject / throttle / log-only) and
/// when to persist the result.
pub fn check_and_increment(
    env: &Env,
    caller: &Address,
    route_call_state: &RouteCallState,
    effective_limit: u32,
    effective_window: u64,
) -> RateLimitCheck {
    let now = env.ledger().timestamp();
    let state: RateLimitState = route_call_state
        .rate_limits
        .get(caller.clone())
        .unwrap_or(RateLimitState {
            calls_in_window: 0,
            window_start: now,
            total_violations: 0,
        });

    let window_elapsed = now >= state.window_start + effective_window;
    let calls = if window_elapsed {
        0
    } else {
        state.calls_in_window
    };
    let window_start = if window_elapsed { now } else { state.window_start };

    if calls >= effective_limit {
        RateLimitCheck {
            exceeded: true,
            updated_state: RateLimitState {
                calls_in_window: calls,
                window_start,
                total_violations: state.total_violations + 1,
            },
        }
    } else {
        RateLimitCheck {
            exceeded: false,
            updated_state: RateLimitState {
                calls_in_window: calls + 1,
                window_start,
                total_violations: state.total_violations,
            },
        }
    }
}

/// Reset the rate limit state for a specific caller on a route.
///
/// No-op if the route has no `RouteCallState` yet.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RouterMiddleware;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::Map;

    fn env_with_contract() -> (Env, Address) {
        let env = Env::default();
        let contract_id = env.register_contract(None, RouterMiddleware);
        (env, contract_id)
    }

    fn empty_state(env: &Env) -> RouteCallState {
        RouteCallState {
            rate_limits: Map::new(env),
            circuit_breaker: crate::CircuitBreakerState {
                failure_count: 0,
                opened_at: 0,
                is_open: false,
                is_half_open: false,
            },
        }
    }

    #[test]
    fn check_and_increment_allows_calls_under_limit() {
        let (env, contract_id) = env_with_contract();
        let caller = Address::generate(&env);
        env.as_contract(&contract_id, || {
            let state = empty_state(&env);
            let check = check_and_increment(&env, &caller, &state, 3, 60);
            assert!(!check.exceeded);
            assert_eq!(check.updated_state.calls_in_window, 1);
        });
    }

    #[test]
    fn check_and_increment_blocks_and_records_violation_over_limit() {
        let (env, contract_id) = env_with_contract();
        let caller = Address::generate(&env);
        env.ledger().set_timestamp(1000);
        env.as_contract(&contract_id, || {
            let mut state = empty_state(&env);
            state.rate_limits.set(
                caller.clone(),
                RateLimitState {
                    calls_in_window: 3,
                    window_start: 1000,
                    total_violations: 0,
                },
            );

            let check = check_and_increment(&env, &caller, &state, 3, 60);
            assert!(check.exceeded);
            assert_eq!(check.updated_state.calls_in_window, 3);
            assert_eq!(check.updated_state.total_violations, 1);
        });
    }

    #[test]
    fn check_and_increment_rolls_window_over_after_elapsed() {
        let (env, contract_id) = env_with_contract();
        let caller = Address::generate(&env);
        env.ledger().set_timestamp(1000);
        env.as_contract(&contract_id, || {
            let mut state = empty_state(&env);
            state.rate_limits.set(
                caller.clone(),
                RateLimitState {
                    calls_in_window: 3,
                    window_start: 1000,
                    total_violations: 0,
                },
            );

            env.ledger().set_timestamp(1060);
            let check = check_and_increment(&env, &caller, &state, 3, 60);
            assert!(!check.exceeded);
            assert_eq!(check.updated_state.calls_in_window, 1);
            assert_eq!(check.updated_state.window_start, 1060);
        });
    }

    #[test]
    fn reset_for_caller_removes_existing_entry() {
        let (env, contract_id) = env_with_contract();
        let route = String::from_str(&env, "route");
        let caller = Address::generate(&env);
        env.as_contract(&contract_id, || {
            let mut state = empty_state(&env);
            state.rate_limits.set(
                caller.clone(),
                RateLimitState {
                    calls_in_window: 2,
                    window_start: 0,
                    total_violations: 0,
                },
            );
            env.storage()
                .instance()
                .set(&DataKey::RouteCallState(route.clone()), &state);

            reset_for_caller(&env, &route, &caller);

            let after: RouteCallState = env
                .storage()
                .instance()
                .get(&DataKey::RouteCallState(route.clone()))
                .unwrap();
            assert!(after.rate_limits.get(caller).is_none());
        });
    }

    #[test]
    fn reset_for_caller_is_noop_when_no_state_exists() {
        let (env, contract_id) = env_with_contract();
        let route = String::from_str(&env, "route");
        let caller = Address::generate(&env);
        env.as_contract(&contract_id, || {
            reset_for_caller(&env, &route, &caller);
            assert!(env
                .storage()
                .instance()
                .get::<DataKey, RouteCallState>(&DataKey::RouteCallState(route.clone()))
                .is_none());
        });
    }
}
