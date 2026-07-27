//! Call event logging and ring-buffer management for router-middleware.
//!
//! Maintains a fixed-capacity ring buffer of [`CallLogEntry`] values per route,
//! updated in [`post_call`], and an incremental [`CallLogSummary`] to avoid
//! reloading all entries for aggregate reads.

use soroban_sdk::{Address, Env, String};

use crate::{CallLogEntry, CallLogState, CallLogSummary, DataKey, RouteConfig};

/// Append a call entry to the ring buffer for `route` and update the summary.
///
/// No-op if `log_retention` is 0 in the route config.
pub fn append(env: &Env, caller: &Address, route: &String, success: bool, config: &RouteConfig) {
    if config.log_retention == 0 {
        return;
    }

    let mut log: CallLogState = env
        .storage()
        .instance()
        .get(&DataKey::CallLog(route.clone()))
        .unwrap_or(CallLogState {
            entries: soroban_sdk::Vec::new(env),
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
        // Growth phase: only hit for routes configured before the
        // pre-allocation upgrade that haven't been re-configured yet.
        log.entries.push_back(entry);
    } else {
        log.entries.set(log.head, entry);
        log.head = (log.head + 1) % cap;
    }
    log.count = log.count.saturating_add(1).min(cap);

    env.storage()
        .instance()
        .set(&DataKey::CallLog(route.clone()), &log);

    // Update summary incrementally
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

/// Clear the call log and summary for `route`.
pub fn clear(env: &Env, route: &String) {
    env.storage()
        .instance()
        .remove(&DataKey::CallLog(route.clone()));
    env.storage()
        .instance()
        .remove(&DataKey::CallLogSummary(route.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RouterMiddleware;
    use soroban_sdk::testutils::Address as _;

    fn env_with_contract() -> (Env, soroban_sdk::Address) {
        let env = Env::default();
        let contract_id = env.register_contract(None, RouterMiddleware);
        (env, contract_id)
    }

    fn cfg(log_retention: u32) -> RouteConfig {
        RouteConfig {
            max_calls_per_window: 0,
            window_seconds: 0,
            enabled: true,
            failure_threshold: 0,
            recovery_window_seconds: 0,
            log_retention,
            burst_allowance: 0,
        }
    }

    #[test]
    fn append_is_noop_when_log_retention_zero() {
        let (env, contract_id) = env_with_contract();
        let caller = soroban_sdk::Address::generate(&env);
        let route = String::from_str(&env, "route");
        env.as_contract(&contract_id, || {
            append(&env, &caller, &route, true, &cfg(0));
            assert!(env
                .storage()
                .instance()
                .get::<DataKey, CallLogState>(&DataKey::CallLog(route.clone()))
                .is_none());
            assert!(env
                .storage()
                .instance()
                .get::<DataKey, CallLogSummary>(&DataKey::CallLogSummary(route.clone()))
                .is_none());
        });
    }

    #[test]
    fn append_updates_summary_counts() {
        let (env, contract_id) = env_with_contract();
        let caller = soroban_sdk::Address::generate(&env);
        let route = String::from_str(&env, "route");
        env.as_contract(&contract_id, || {
            append(&env, &caller, &route, true, &cfg(5));
            append(&env, &caller, &route, false, &cfg(5));
            let summary: CallLogSummary = env
                .storage()
                .instance()
                .get(&DataKey::CallLogSummary(route.clone()))
                .unwrap();
            assert_eq!(summary.total_calls, 2);
            assert_eq!(summary.success_count, 1);
            assert_eq!(summary.failure_count, 1);
        });
    }

    #[test]
    fn clear_removes_both_log_and_summary() {
        // Regression test for issue #812: reset must not leave a stale
        // CallLogSummary behind after the CallLog itself is cleared.
        let (env, contract_id) = env_with_contract();
        let caller = soroban_sdk::Address::generate(&env);
        let route = String::from_str(&env, "route");
        env.as_contract(&contract_id, || {
            append(&env, &caller, &route, true, &cfg(5));
            assert!(env
                .storage()
                .instance()
                .has(&DataKey::CallLogSummary(route.clone())));
            assert!(env.storage().instance().has(&DataKey::CallLog(route.clone())));

            clear(&env, &route);

            assert!(!env
                .storage()
                .instance()
                .has(&DataKey::CallLogSummary(route.clone())));
            assert!(!env.storage().instance().has(&DataKey::CallLog(route.clone())));
        });
    }
}
