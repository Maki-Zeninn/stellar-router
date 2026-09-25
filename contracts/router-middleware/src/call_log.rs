//! Call log module for router-middleware.
//!
//! Records per-route call events with a fixed-capacity ring buffer and an
//! incrementally maintained summary.

use soroban_sdk::{Address, Env, String, Vec};

use crate::{CallLogEntry, CallLogState, CallLogSummary, DataKey};

/// Append a call entry to the route's log, evicting the oldest entry when the
/// ring buffer is full, and update the route's summary counters.
pub fn record(env: &Env, route: &String, caller: &Address, success: bool) {
    let key = DataKey::CallLog(route.clone());
    let mut state: CallLogState = match env.storage().persistent().get(&key) {
        Some(s) => s,
        None => return,
    };

    let capacity = state.entries.len();
    if capacity == 0 {
        return;
    }

    let entry = CallLogEntry {
        caller: caller.clone(),
        timestamp: env.ledger().timestamp(),
        success,
        route: route.clone(),
    };

    if state.count < capacity {
        state.entries.set(state.count, entry);
        state.count += 1;
    } else {
        state.entries.set(state.head, entry);
        state.head = (state.head + 1) % capacity;
    }

    env.storage().persistent().set(&key, &state);

    // Update summary incrementally.
    let summary_key = DataKey::CallLogSummary(route.clone());
    let mut summary: CallLogSummary = env
        .storage()
        .persistent()
        .get(&summary_key)
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
    env.storage().persistent().set(&summary_key, &summary);
}

/// Wipe the accumulated call-log state for a route, returning it to its
/// initial/empty form. Removes both the ring buffer and the summary.
pub fn reset(env: &Env, route: &String) {
    env.storage()
        .persistent()
        .remove(&DataKey::CallLog(route.clone()));
    env.storage()
        .persistent()
        .remove(&DataKey::CallLogSummary(route.clone()));
}

/// Read the retained call entries for a route, oldest first.
pub fn get_entries(env: &Env, route: &String) -> Vec<CallLogEntry> {
    let key = DataKey::CallLog(route.clone());
    let state: CallLogState = match env.storage().persistent().get(&key) {
        Some(s) => s,
        None => return Vec::new(env),
    };

    let capacity = state.entries.len();
    let mut out = Vec::new(env);
    if state.count < capacity {
        for i in 0..state.count {
            out.push_back(state.entries.get(i).unwrap());
        }
    } else {
        for i in 0..capacity {
            let idx = (state.head + i) % capacity;
            out.push_back(state.entries.get(idx).unwrap());
        }
    }
    out
}

/// Read the aggregated summary for a route's call log.
pub fn get_summary(env: &Env, route: &String) -> CallLogSummary {
    env.storage()
        .persistent()
        .get(&DataKey::CallLogSummary(route.clone()))
        .unwrap_or(CallLogSummary {
            total_calls: 0,
            success_count: 0,
            failure_count: 0,
            last_call_timestamp: 0,
        })
}
