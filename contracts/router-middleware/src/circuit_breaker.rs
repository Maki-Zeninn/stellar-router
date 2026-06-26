//! Circuit breaker state machine for router-middleware.
//!
//! Tracks per-route failure counts and manages open/half-open/closed
//! transitions. Logic is called from [`pre_call`] and [`post_call`]
//! in `lib.rs`.

use soroban_sdk::{Env, Map, String, Symbol};

use crate::{CircuitBreakerState, DataKey, RouteCallState, RouteConfig};

/// Check circuit breaker state in `pre_call` and transition to half-open if
/// the recovery window has elapsed. Returns `true` if the call should be
/// blocked (circuit is open and recovery window has not elapsed).
pub fn check_and_transition(
    env: &Env,
    _route: &String,
    config: &RouteConfig,
    route_call_state: &mut RouteCallState,
) -> bool {
    if route_call_state.circuit_breaker.is_open {
        let recovers = config.recovery_window_seconds > 0
            && env.ledger().timestamp()
                >= route_call_state.circuit_breaker.opened_at + config.recovery_window_seconds;

        if recovers {
            route_call_state.circuit_breaker.is_open = false;
            route_call_state.circuit_breaker.is_half_open = true;
            false
        } else {
            true // still blocked
        }
    } else {
        false
    }
}

/// Handle a failure in `post_call`: increment failure count and open circuit
/// if threshold is reached. Also handles the half-open re-open case.
pub fn handle_failure(
    env: &Env,
    route: &String,
    config: &RouteConfig,
    route_call_state: &mut RouteCallState,
) {
    if route_call_state.circuit_breaker.is_half_open {
        // Probe failed — reopen the circuit
        route_call_state.circuit_breaker.is_half_open = false;
        route_call_state.circuit_breaker.is_open = true;
        route_call_state.circuit_breaker.opened_at = env.ledger().timestamp();
        route_call_state.circuit_breaker.failure_count = 1;
        env.events().publish(
            (Symbol::new(env, "circuit_opened"),),
            (
                route.clone(),
                route_call_state.circuit_breaker.failure_count,
            ),
        );
    } else {
        route_call_state.circuit_breaker.failure_count += 1;
        if route_call_state.circuit_breaker.failure_count >= config.failure_threshold {
            route_call_state.circuit_breaker.is_open = true;
            route_call_state.circuit_breaker.opened_at = env.ledger().timestamp();
            env.events().publish(
                (Symbol::new(env, "circuit_opened"),),
                (
                    route.clone(),
                    route_call_state.circuit_breaker.failure_count,
                ),
            );
        }
    }
}

/// Handle a success in `post_call`: close circuit if in half-open state,
/// or reset failure count if failures exist.
pub fn handle_success(route_call_state: &mut RouteCallState) {
    if route_call_state.circuit_breaker.is_half_open {
        route_call_state.circuit_breaker.is_half_open = false;
        route_call_state.circuit_breaker.failure_count = 0;
    } else if !route_call_state.circuit_breaker.is_open
        && route_call_state.circuit_breaker.failure_count > 0
    {
        route_call_state.circuit_breaker.failure_count = 0;
    }
}

/// Reset the circuit breaker for a route back to closed state.
pub fn reset(env: &Env, route: &String) {
    let existing: Option<RouteCallState> = env
        .storage()
        .instance()
        .get(&DataKey::RouteCallState(route.clone()));

    if let Some(mut state) = existing {
        state.circuit_breaker = CircuitBreakerState {
            failure_count: 0,
            opened_at: 0,
            is_open: false,
            is_half_open: false,
        };
        env.storage()
            .instance()
            .set(&DataKey::RouteCallState(route.clone()), &state);
    }
}

/// Return a default `RouteCallState` with a closed circuit breaker.
pub fn default_route_call_state(env: &Env) -> RouteCallState {
    RouteCallState {
        rate_limits: Map::new(env),
        circuit_breaker: CircuitBreakerState {
            failure_count: 0,
            opened_at: 0,
            is_open: false,
            is_half_open: false,
        },
    }
}
