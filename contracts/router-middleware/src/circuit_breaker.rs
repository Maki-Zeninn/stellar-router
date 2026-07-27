//! Circuit breaker state machine for router-middleware.
//!
//! Tracks per-route failure counts and manages open/half-open/closed
//! transitions. Logic is called from [`pre_call`] and [`post_call`]
//! in `lib.rs`.

use router_common;
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
            (Symbol::new(env, router_common::EVENT_CIRCUIT_OPENED),),
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
                (Symbol::new(env, router_common::EVENT_CIRCUIT_OPENED),),
                (
                    route.clone(),
                    route_call_state.circuit_breaker.failure_count,
                ),
            );
        }
    }
}

/// Handle a success in `post_call`: close circuit if in half-open state
/// (emitting a `circuit_closed` event and clearing `opened_at`), or reset
/// the failure count if failures exist.
pub fn handle_success(env: &Env, route: &String, route_call_state: &mut RouteCallState) {
    if route_call_state.circuit_breaker.is_half_open {
        route_call_state.circuit_breaker.is_half_open = false;
        route_call_state.circuit_breaker.failure_count = 0;
        route_call_state.circuit_breaker.opened_at = 0;
        env.events().publish(
            (Symbol::new(env, router_common::EVENT_CIRCUIT_CLOSED),),
            route.clone(),
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RouterMiddleware;
    use soroban_sdk::testutils::{Events, Ledger};
    use soroban_sdk::IntoVal;

    fn env_with_contract() -> (Env, soroban_sdk::Address) {
        let env = Env::default();
        let contract_id = env.register_contract(None, RouterMiddleware);
        (env, contract_id)
    }

    fn cfg(failure_threshold: u32, recovery_window_seconds: u64) -> RouteConfig {
        RouteConfig {
            max_calls_per_window: 0,
            window_seconds: 0,
            enabled: true,
            failure_threshold,
            recovery_window_seconds,
            log_retention: 0,
            burst_allowance: 0,
        }
    }

    #[test]
    fn handle_failure_opens_circuit_at_threshold() {
        let (env, contract_id) = env_with_contract();
        let route = String::from_str(&env, "route");
        env.as_contract(&contract_id, || {
            let mut state = default_route_call_state(&env);
            let config = cfg(2, 60);
            handle_failure(&env, &route, &config, &mut state);
            assert!(!state.circuit_breaker.is_open);
            assert_eq!(state.circuit_breaker.failure_count, 1);

            handle_failure(&env, &route, &config, &mut state);
            assert!(state.circuit_breaker.is_open);
            assert_eq!(state.circuit_breaker.failure_count, 2);
        });
    }

    #[test]
    fn check_and_transition_blocks_while_open_and_recovery_window_not_elapsed() {
        let (env, contract_id) = env_with_contract();
        let route = String::from_str(&env, "route");
        env.ledger().set_timestamp(1000);
        env.as_contract(&contract_id, || {
            let mut state = default_route_call_state(&env);
            state.circuit_breaker.is_open = true;
            state.circuit_breaker.opened_at = 1000;
            let config = cfg(1, 60);

            let blocked = check_and_transition(&env, &route, &config, &mut state);
            assert!(blocked);
            assert!(state.circuit_breaker.is_open);
        });
    }

    #[test]
    fn check_and_transition_moves_to_half_open_after_recovery_window() {
        let (env, contract_id) = env_with_contract();
        let route = String::from_str(&env, "route");
        env.ledger().set_timestamp(1000);
        env.as_contract(&contract_id, || {
            let mut state = default_route_call_state(&env);
            state.circuit_breaker.is_open = true;
            state.circuit_breaker.opened_at = 1000;
            let config = cfg(1, 60);

            env.ledger().set_timestamp(1060);
            let blocked = check_and_transition(&env, &route, &config, &mut state);
            assert!(!blocked);
            assert!(!state.circuit_breaker.is_open);
            assert!(state.circuit_breaker.is_half_open);
        });
    }

    #[test]
    fn handle_success_closes_circuit_from_half_open_and_emits_event() {
        let (env, contract_id) = env_with_contract();
        let route = String::from_str(&env, "route");
        env.as_contract(&contract_id, || {
            let mut state = default_route_call_state(&env);
            state.circuit_breaker.is_half_open = true;
            state.circuit_breaker.opened_at = 500;
            state.circuit_breaker.failure_count = 3;

            handle_success(&env, &route, &mut state);

            assert!(!state.circuit_breaker.is_half_open);
            assert_eq!(state.circuit_breaker.failure_count, 0);
            assert_eq!(state.circuit_breaker.opened_at, 0);
        });

        let events = env.events().all();
        let expected_topic = Symbol::new(&env, router_common::EVENT_CIRCUIT_CLOSED);
        let found = events.iter().any(|e| {
            let topic: Symbol = e.1.get(0).unwrap().into_val(&env);
            topic == expected_topic
        });
        assert!(found, "circuit_closed event must be emitted");
    }

    #[test]
    fn reset_clears_open_circuit_back_to_closed() {
        let (env, contract_id) = env_with_contract();
        let route = String::from_str(&env, "route");
        env.as_contract(&contract_id, || {
            let mut state = default_route_call_state(&env);
            state.circuit_breaker.is_open = true;
            state.circuit_breaker.failure_count = 5;
            state.circuit_breaker.opened_at = 42;
            env.storage()
                .instance()
                .set(&DataKey::RouteCallState(route.clone()), &state);

            reset(&env, &route);

            let after: RouteCallState = env
                .storage()
                .instance()
                .get(&DataKey::RouteCallState(route.clone()))
                .unwrap();
            assert!(!after.circuit_breaker.is_open);
            assert_eq!(after.circuit_breaker.failure_count, 0);
            assert_eq!(after.circuit_breaker.opened_at, 0);
        });
    }

    #[test]
    fn reset_is_noop_when_no_state_exists() {
        let (env, contract_id) = env_with_contract();
        let route = String::from_str(&env, "route");
        env.as_contract(&contract_id, || {
            reset(&env, &route);
            assert!(env
                .storage()
                .instance()
                .get::<DataKey, RouteCallState>(&DataKey::RouteCallState(route.clone()))
                .is_none());
        });
    }
}
