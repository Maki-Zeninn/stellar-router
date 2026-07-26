//! Route scoring and best-route selection logic.
//!
//! Provides the [`RouteScore`] type and the [`recompute_best_route`] helper
//! that scans all routes and caches the highest-scoring, non-paused route
//! under [`DataKey::BestRoute`].

use soroban_sdk::{Env, String, Symbol};

use crate::{is_route_expired, DataKey, RouteEntry, RouteScore, ScoringWeights};

/// Recompute and cache the highest-scoring, non-paused route.
///
/// Performs a single O(n) scan over all routes and stores the winner under
/// [`DataKey::BestRoute`] (or removes the key when no scored, non-paused
/// route exists). Called from every write path that can change the outcome:
/// scoring, pausing, and route removal.
///
/// The composite formula uses configurable weights stored under
/// [`DataKey::ScoringWeights`]:
/// `liquidity_weight * liquidity_score + reliability_weight * reliability_score - fee_bps / fee_divisor`
///
/// Falls back to `(1, 1, 10)` when no weights have been explicitly set.
pub fn recompute_best_route(env: &Env) {
    let names: soroban_sdk::Vec<String> = env
        .storage()
        .instance()
        .get(&DataKey::RouteNames)
        .unwrap_or(soroban_sdk::Vec::new(env));

    let mut best_name: Option<String> = None;
    let mut best_score: i64 = i64::MIN;

    // Load configurable weights (falls back to defaults when not set).
    let weights: ScoringWeights = env
        .storage()
        .instance()
        .get::<DataKey, ScoringWeights>(&DataKey::ScoringWeights)
        .unwrap_or(ScoringWeights {
            liquidity_weight: 1,
            reliability_weight: 1,
            fee_divisor: 10,
        });

    for name in names.iter() {
        // Skip missing, paused, or expired routes
        match env
            .storage()
            .instance()
            .get::<DataKey, RouteEntry>(&DataKey::Route(name.clone()))
        {
            Some(e) if !e.paused && !is_route_expired(env, &e) => {}
            _ => continue,
        }

        // Skip routes without a score
        let score: RouteScore = match env.storage().instance().get(&DataKey::Score(name.clone())) {
            Some(s) => s,
            None => continue,
        };

        // Composite score using configurable weights:
        //   liquidity_weight * liquidity_score
        // + reliability_weight * reliability_score
        // - fee_bps / fee_divisor
        let composite: i64 = weights.liquidity_weight * (score.liquidity_score as i64)
            + weights.reliability_weight * (score.reliability_score as i64)
            - (score.fee_bps as i64 / weights.fee_divisor);

        if composite > best_score {
            best_score = composite;
            best_name = Some(name.clone());
        }
    }

    match best_name {
        Some(name) => {
            env.storage().instance().set(&DataKey::BestRoute, &name);
            env.events().publish(
                (Symbol::new(env, router_common::EVENT_BEST_ROUTE_SELECTED),),
                (name, best_score),
            );
        }
        None => env.storage().instance().remove(&DataKey::BestRoute),
    }
}
