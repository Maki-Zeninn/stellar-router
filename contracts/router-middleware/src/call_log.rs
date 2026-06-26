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
