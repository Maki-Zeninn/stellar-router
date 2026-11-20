# Issue #1674 — `recompute_best_route` Emits `best_route_selected` Even When the Best Route Didn't Change

**Tracking:** [#1674](https://github.com/Maki-Zeninn/stellar-router/issues/1674)  
**Component:** `contracts/router-core`  
**Type:** Bug  
**Impact:** Indexers and alerting systems that treat `best_route_selected` as "routing target changed" receive duplicate events that carry no new information

---

## Summary

`scoring::recompute_best_route` runs on every write that could affect route selection:
score updates, pause/unpause, and route removal. After computing the winner it
unconditionally stores it and publishes `best_route_selected(name, best_score)`, even
when the winner is exactly the same as the previously cached `DataKey::BestRoute`.

This means that pausing an unrelated low-score route, or updating a non-best route's
score, re-announces the same winner. Consumers that use `best_route_selected` to detect
routing-target changes — indexers, monitors, alerting pipelines — are flooded with
no-op events and cannot distinguish real transitions from spurious re-announcements.

---

## Location

**File:** `contracts/router-core/src/scoring.rs`

```rust
pub fn recompute_best_route(env: &Env) {
    let names: soroban_sdk::Vec<String> = env
        .storage().instance()
        .get(&DataKey::RouteNames)
        .unwrap_or(soroban_sdk::Vec::new(env));

    let mut best_name: Option<String> = None;
    let mut best_score: i64 = i64::MIN;

    // ... scoring loop omitted for brevity ...

    match best_name {
        Some(name) => {
            // ❌ Always writes and always publishes, regardless of whether
            //    'name' differs from the currently cached BestRoute.
            env.storage().instance().set(&DataKey::BestRoute, &name);
            env.events().publish(
                (Symbol::new(env, router_common::EVENT_BEST_ROUTE_SELECTED),),
                (name, best_score),
            );
        }
        None => env.storage().instance().remove(&DataKey::BestRoute),
    }
}
```

---

## Affected Triggers

`recompute_best_route` is called from every write path that can change the outcome:

| Caller | When it fires |
|---|---|
| `set_score` / `update_score` | Any route's score changes |
| `pause_route` / `unpause_route` | Any route is paused or resumed |
| `remove_route` | Any route is deleted |
| `set_alias` / `remove_alias` | Alias table changes |

All of these trigger a recompute and, under the current code, an unconditional publish.
Only a subset of these operations can actually change which route wins.

---

## Required Change

Before writing and publishing, read the current `DataKey::BestRoute` and compare it to
the newly computed winner. Publish only when the value changes.

```rust
pub fn recompute_best_route(env: &Env) {
    let names: soroban_sdk::Vec<String> = env
        .storage().instance()
        .get(&DataKey::RouteNames)
        .unwrap_or(soroban_sdk::Vec::new(env));

    let mut best_name: Option<String> = None;
    let mut best_score: i64 = i64::MIN;

    let weights: ScoringWeights = env
        .storage().instance()
        .get::<DataKey, ScoringWeights>(&DataKey::ScoringWeights)
        .unwrap_or(crate::DEFAULT_SCORING_WEIGHTS);

    for name in names.iter() {
        match env.storage().instance()
            .get::<DataKey, RouteEntry>(&DataKey::Route(name.clone()))
        {
            Some(e) if !e.paused && !is_route_expired(env, &e) => {}
            _ => continue,
        }
        let score: RouteScore = match env.storage().instance()
            .get(&DataKey::Score(name.clone()))
        {
            Some(s) => s,
            None => continue,
        };
        let composite: i64 =
            weights.liquidity_weight * (score.liquidity_score as i64)
            + weights.reliability_weight * (score.reliability_score as i64)
            - (score.fee_bps as i64 / weights.fee_divisor);
        if composite > best_score {
            best_score = composite;
            best_name = Some(name.clone());
        }
    }

    match best_name {
        Some(name) => {
            // ✅ Read previous winner; only write and publish when it changes.
            let previous: Option<String> = env
                .storage().instance()
                .get(&DataKey::BestRoute);
            if previous.as_ref() != Some(&name) {
                env.storage().instance().set(&DataKey::BestRoute, &name);
                env.events().publish(
                    (Symbol::new(env, router_common::EVENT_BEST_ROUTE_SELECTED),),
                    (name, best_score),
                );
            }
        }
        None => env.storage().instance().remove(&DataKey::BestRoute),
    }
}
```

---

## Behaviour After Fix

| Action | Best route changes? | `best_route_selected` emitted? |
|---|---|---|
| Update a non-best route's score | No | No ✅ |
| Pause a non-best route | No | No ✅ |
| Update the best route's score (still wins) | No | No ✅ |
| Update a non-best route's score so it now wins | Yes | Yes ✅ |
| Pause the best route | Yes (new winner) | Yes ✅ |
| Remove the best route | Yes (new winner or none) | Yes ✅ |

---

## Tests Required

**Non-best route update does not emit:**
```rust
#[test]
fn test_recompute_does_not_emit_when_best_route_unchanged() {
    let (env, admin, client) = setup();
    // Register two routes; route-a has the higher score (wins).
    // Update route-b's score (route-a still wins).
    // Assert that no best_route_selected event was emitted for the update.
    let events = env.events().all();
    let best_route_events: Vec<_> = events.iter()
        .filter(|e| /* topic matches EVENT_BEST_ROUTE_SELECTED */)
        .collect();
    // Only the initial scoring emit is present; the second update produced none.
    assert_eq!(best_route_events.len(), 1);
}
```

**Winner change does emit:**
```rust
#[test]
fn test_recompute_emits_when_best_route_changes() {
    let (env, admin, client) = setup();
    // route-a wins initially.
    // Update route-b's score so it exceeds route-a.
    // Assert that a new best_route_selected event was emitted for route-b.
    let events = env.events().all();
    let last_event = /* last best_route_selected event */;
    assert_eq!(last_event.name, "route-b");
}
```

---

## Acceptance Criteria

- [ ] Changing a non-best route's score does not emit `best_route_selected`.
- [ ] Pausing or unpausing a route that is not the best does not emit `best_route_selected`.
- [ ] Changing the winning route's score or pausing the winner does emit
  `best_route_selected` with the new winner.
- [ ] The storage write to `DataKey::BestRoute` is also skipped when the winner hasn't changed.
- [ ] Tests cover both the unchanged case and the changed case.
- [ ] No existing tests are broken.
