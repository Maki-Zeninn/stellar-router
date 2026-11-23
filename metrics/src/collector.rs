//! Background scrape loop.
//!
//! The [`Collector`] spawns a `tokio` task that wakes up every
//! `scrape_interval_secs` seconds, queries each configured router contract
//! via the Soroban RPC, and updates the Prometheus gauges / counters.
//!
//! ## Scraping strategy
//!
//! Soroban contracts store state in on-chain ledger entries.  The cleanest
//! way to read that state from off-chain is to call the contract's view
//! functions via `simulateTransaction`.  This exporter calls:
//!
//! - `router-core`: `total_routed()`, `is_paused()`, `get_all_routes()`
//!   + `get_route(name)` for each route
//! - `router-middleware`: `total_calls()`, `get_configured_routes()`
//!   + `circuit_breaker_state(route)` for each route
//! - `router-registry`: `get_all_names()` (total count)
//!
//! Each contract scrape is timed and any error increments the
//! `router_scrape_errors_total` counter for that contract label.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use futures::{stream, StreamExt};
use tracing::{error, info, warn};

use crate::cli::Args;
use crate::metrics::RouterMetrics;
use crate::rpc::{ContractEvent, RpcClient, SorobanRpcClient};

/// Drives the periodic scrape loop.
#[derive(Clone)]
pub struct Collector {
    args: Args,
    metrics: RouterMetrics,
    /// Cursor state for quote events, keyed by (contract_id, topic).
    quote_cursors: Arc<Mutex<HashMap<(String, String), String>>>,
}

impl Collector {
    pub fn new(args: Args, metrics: RouterMetrics) -> Self {
        Self {
            args,
            metrics,
            quote_cursors: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Run forever, scraping on the configured interval.
    pub async fn run(self) {
        let interval = tokio::time::Duration::from_secs(self.args.scrape_interval_secs);
        info!(
            interval_secs = self.args.scrape_interval_secs,
            "scrape loop started"
        );

        let client = match SorobanRpcClient::new(&self.args.rpc_url, self.args.rpc_timeout_secs) {
            Ok(c) => c,
            Err(e) => {
                error!("failed to create RPC client: {e:#}");
                return;
            }
        };
        self.metrics.rpc_up.set(1.0);

        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let cycle_ok = self.scrape_all(&client).await;
            self.metrics.up.set(if cycle_ok { 1.0 } else { 0.0 });
            self.metrics.scrape_cycle_completed.set(1.0);
            tokio::time::sleep(interval).await;
        }
    }

    /// Scrape all configured contracts.  Returns `true` if every scrape
    /// succeeded, `false` if any failed.
    async fn scrape_all(&self, client: &dyn RpcClient) -> bool {
        let (core_ok, middleware_ok, registry_ok, quote_ok, execution_ok) = tokio::join!(
            self.scrape_and_record(
                &self.args.core_contract_id,
                "core",
                self.scrape_core(client, &self.args.core_contract_id),
            ),
            self.scrape_and_record(
                &self.args.middleware_contract_id,
                "middleware",
                self.scrape_middleware(client, &self.args.middleware_contract_id),
            ),
            self.scrape_and_record(
                &self.args.registry_contract_id,
                "registry",
                self.scrape_registry(client, &self.args.registry_contract_id),
            ),
            self.scrape_and_record(
                &self.args.quote_contract_id,
                "quote",
                self.scrape_quote(client, &self.args.quote_contract_id),
            ),
            self.scrape_and_record(
                &self.args.execution_contract_id,
                "execution",
                self.scrape_execution(client, &self.args.execution_contract_id),
            ),
        );

        core_ok && middleware_ok && registry_ok && quote_ok && execution_ok
    }

    /// Scrape a single contract and record the outcome: a no-op success when
    /// `contract_id` is unconfigured, or on failure a warn log plus an
    /// increment of `scrape_errors_total`. Returns `true` if the scrape
    /// succeeded (or was skipped), `false` if it failed.
    async fn scrape_and_record(
        &self,
        contract_id: &str,
        label: &str,
        scrape: impl std::future::Future<Output = Result<()>>,
    ) -> bool {
        if contract_id.is_empty() {
            return true;
        }
        if let Err(e) = scrape.await {
            warn!(contract_id, "{label} scrape failed: {e:#}");
            self.metrics
                .contract_up
                .with_label_values(&[contract_id])
                .set(0.0);
            self.metrics
                .scrape_errors_total
                .with_label_values(&[contract_id])
                .inc();
            return false;
        }
        self.metrics
            .contract_up
            .with_label_values(&[contract_id])
            .set(1.0);
        true
    }

    // ── router-core ───────────────────────────────────────────────────────────

    async fn scrape_core(&self, client: &dyn RpcClient, contract_id: &str) -> Result<()> {
        let start = Instant::now();
        info!(contract_id, "scraping router-core");

        // 1. total_routed
        let total_routed = client.call_u64(contract_id, "total_routed").await?;
        self.metrics
            .core_total_routed
            .with_label_values(&[contract_id])
            .set(total_routed as f64);

        // 2. is_paused
        //
        // Prefer the dedicated bool view when the contract exposes it. If the
        // RPC call fails, leave the metric at 0.0 so the exporter does not
        // report a false positive from a fallback path.
        let paused_value = match client.call_bool(contract_id, "is_paused").await {
            Ok(paused) => {
                if paused {
                    1.0
                } else {
                    0.0
                }
            }
            Err(e) => {
                warn!(
                    contract_id,
                    "failed to read router pause state via is_paused: {e:#}"
                );
                0.0
            }
        };
        self.metrics
            .core_paused
            .with_label_values(&[contract_id])
            .set(paused_value);

        // 3. get_all_routes → per-route paused state
        let routes = client
            .call_string_vec(contract_id, "get_all_routes")
            .await?;
        stream::iter(routes.iter().cloned())
            .map(|route| async move {
                let route_result = client
                    .simulate_invoke(contract_id, "get_route", vec![encode_string_arg(&route)])
                    .await;
                (route, route_result)
            })
            .buffer_unordered(self.args.scrape_concurrency.max(1))
            .for_each(|(route, route_result)| async {
                match route_result {
                    Ok(val) => {
                        let paused = extract_route_paused(&val).unwrap_or(false);
                        self.metrics
                            .core_route_paused
                            .with_label_values(&[contract_id, &route])
                            .set(if paused { 1.0 } else { 0.0 });
                    }
                    Err(e) => {
                        warn!(contract_id, route, "failed to get route state: {e:#}");
                    }
                }
            })
            .await;

        let elapsed = start.elapsed().as_secs_f64();
        self.metrics
            .scrape_duration_seconds
            .with_label_values(&[contract_id])
            .observe(elapsed);

        info!(
            contract_id,
            elapsed_secs = elapsed,
            routes = routes.len(),
            total_routed,
            "core scrape done"
        );
        Ok(())
    }

    // ── router-middleware ─────────────────────────────────────────────────────

    async fn scrape_middleware(&self, client: &dyn RpcClient, contract_id: &str) -> Result<()> {
        let start = Instant::now();
        info!(contract_id, "scraping router-middleware");

        // 1. total_calls
        let total_calls = client.call_u64(contract_id, "total_calls").await?;
        self.metrics
            .middleware_total_calls
            .with_label_values(&[contract_id])
            .set(total_calls as f64);

        // 2. Per-route circuit breaker state
        let routes = client
            .call_string_vec(contract_id, "get_configured_routes")
            .await?;

        stream::iter(routes.iter().cloned())
            .map(|route| async move {
                let cb_result = client
                    .simulate_invoke(
                        contract_id,
                        "circuit_breaker_state",
                        vec![encode_string_arg(&route)],
                    )
                    .await;
                (route, cb_result)
            })
            .buffer_unordered(self.args.scrape_concurrency.max(1))
            .for_each(|(route, cb_result)| async {
                match cb_result {
                    Ok(val) => {
                        let (is_open, failure_count) = extract_circuit_breaker_state(&val);
                        self.metrics
                            .middleware_circuit_open
                            .with_label_values(&[contract_id, &route])
                            .set(if is_open { 1.0 } else { 0.0 });
                        self.metrics
                            .middleware_failure_count
                            .with_label_values(&[contract_id, &route])
                            .set(failure_count as f64);
                    }
                    Err(e) => {
                        warn!(
                            contract_id,
                            route, "failed to get circuit breaker state: {e:#}"
                        );
                    }
                }
            })
            .await;

        let elapsed = start.elapsed().as_secs_f64();
        self.metrics
            .scrape_duration_seconds
            .with_label_values(&[contract_id])
            .observe(elapsed);

        info!(
            contract_id,
            elapsed_secs = elapsed,
            routes = routes.len(),
            total_calls,
            "middleware scrape done"
        );
        Ok(())
    }

    // ── router-registry ───────────────────────────────────────────────────────

    async fn scrape_registry(&self, client: &dyn RpcClient, contract_id: &str) -> Result<()> {
        let start = Instant::now();
        info!(contract_id, "scraping router-registry");

        // get_all_names returns Vec<String> of registered contract names
        let names = client.call_string_vec(contract_id, "get_all_names").await?;

        self.metrics
            .registry_total_names
            .with_label_values(&[contract_id])
            .set(names.len() as f64);

        let elapsed = start.elapsed().as_secs_f64();
        self.metrics
            .scrape_duration_seconds
            .with_label_values(&[contract_id])
            .observe(elapsed);

        info!(
            contract_id,
            elapsed_secs = elapsed,
            total_names = names.len(),
            "registry scrape done"
        );
        Ok(())
    }

    // ── router-quote ──────────────────────────────────────────────────────────

    /// Scrape `router-quote` by counting `quote_calculated` and `fee_estimated`
    /// events via the `getEvents` RPC and maintaining running totals.
    async fn scrape_quote(&self, client: &dyn RpcClient, contract_id: &str) -> Result<()> {
        let start = Instant::now();
        info!(contract_id, "scraping router-quote");

        let quote_events = self
            .fetch_all_events(client, contract_id, "quote_calculated")
            .await?;
        let fee_events = self
            .fetch_all_events(client, contract_id, "fee_estimated")
            .await?;

        if !quote_events.is_empty() {
            self.metrics
                .quote_total_generated
                .with_label_values(&[contract_id])
                .inc_by(quote_events.len() as f64);
        }

        if !fee_events.is_empty() {
            self.metrics
                .quote_total_fee_estimated
                .with_label_values(&[contract_id])
                .inc_by(fee_events.len() as f64);
        }

        let elapsed = start.elapsed().as_secs_f64();
        self.metrics
            .scrape_duration_seconds
            .with_label_values(&[contract_id])
            .observe(elapsed);

        info!(
            contract_id,
            elapsed_secs = elapsed,
            quote_calculated = quote_events.len(),
            fee_estimated = fee_events.len(),
            "quote scrape done"
        );
        Ok(())
    }

    /// Fetch all events for a given topic, following pagination until the page is empty.
    /// Maintains a cursor per (contract, topic) to only fetch new events each cycle.
    async fn fetch_all_events(
        &self,
        client: &dyn RpcClient,
        contract_id: &str,
        topic: &str,
    ) -> Result<Vec<ContractEvent>> {
        let key = (contract_id.to_string(), topic.to_string());
        let mut cursor = {
            let cursors = self.quote_cursors.lock().unwrap();
            cursors.get(&key).cloned()
        };

        let mut all_events = Vec::new();
        loop {
            let (events, next_cursor) = client
                .get_events(contract_id, &[topic], cursor.as_deref())
                .await?;

            if events.is_empty() {
                break;
            }

            all_events.extend(events.clone());

            let page_cursor = next_cursor
                .or_else(|| events.last().map(|e| e.id.clone()))
                .filter(|c| !c.is_empty());

            if page_cursor.is_none() || page_cursor.as_ref() == cursor.as_ref() {
                cursor = page_cursor;
                break;
            }

            cursor = page_cursor;
        }

        if let Some(ref c) = cursor {
            let mut cursors = self.quote_cursors.lock().unwrap();
            cursors.insert(key, c.clone());
        }

        Ok(all_events)
    }

    // ── router-execution ──────────────────────────────────────────────────────

    /// Scrape `router-execution` by calling its `stats()` view function
    /// (`TotalExecutions`, `TotalErrors`) and `max_retries()`.
    ///
    /// A value that cannot be read fails the scrape instead of being reported
    /// as 0.
    async fn scrape_execution(&self, client: &dyn RpcClient, contract_id: &str) -> Result<()> {
        let start = Instant::now();
        info!(contract_id, "scraping router-execution");

        let stats = client.simulate_invoke(contract_id, "stats", vec![]).await?;
        let (total_executions, total_errors) = extract_execution_stats(&stats)?;
        let max_retries = client.call_u64(contract_id, "max_retries").await?;

        self.metrics
            .execution_total_executions
            .with_label_values(&[contract_id])
            .set(total_executions as f64);

        self.metrics
            .execution_total_errors
            .with_label_values(&[contract_id])
            .set(total_errors as f64);

        self.metrics
            .execution_max_retries
            .with_label_values(&[contract_id])
            .set(max_retries as f64);

        let elapsed = start.elapsed().as_secs_f64();
        self.metrics
            .scrape_duration_seconds
            .with_label_values(&[contract_id])
            .observe(elapsed);

        info!(
            contract_id,
            elapsed_secs = elapsed,
            total_executions,
            total_errors,
            max_retries,
            "execution scrape done"
        );
        Ok(())
    }
}

/// Encode a plain string as a base64 XDR `ScVal::String` argument.
///
/// This is a placeholder — a real implementation would use the `stellar-xdr`
/// crate to produce the correct XDR encoding.
fn encode_string_arg(s: &str) -> String {
    // Base64-encode the raw UTF-8 bytes as a minimal stand-in.
    // Replace with proper ScVal XDR encoding in production.
    use std::fmt::Write;
    let mut out = String::new();
    for b in s.as_bytes() {
        write!(out, "{b:02x}").ok();
    }
    out
}

/// Extract the `paused` field from a `RouteEntry` JSON value returned by
/// `simulateTransaction`.
fn extract_route_paused(val: &serde_json::Value) -> Option<bool> {
    // The Soroban RPC returns struct fields as a JSON map.
    // RouteEntry { address, name, paused, updated_by, metadata }
    val.get("results")
        .and_then(|r| r.get(0))
        .and_then(|r| r.get("retval"))
        .and_then(|v| v.get("paused"))
        .and_then(|p| p.as_bool())
        .or_else(|| val.get("paused").and_then(|p| p.as_bool()))
}

/// Extract `(is_open, failure_count)` from a `CircuitBreakerState` JSON value.
fn extract_circuit_breaker_state(val: &serde_json::Value) -> (bool, u32) {
    let retval = val
        .get("results")
        .and_then(|r| r.get(0))
        .and_then(|r| r.get("retval"))
        .unwrap_or(val);

    // Handle Option<CircuitBreakerState> — None means no state recorded yet
    if retval.is_null() || retval.get("none").is_some() {
        return (false, 0);
    }

    let state = retval.get("some").unwrap_or(retval);
    let is_open = state
        .get("is_open")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let failure_count = state
        .get("failure_count")
        .and_then(|v| v.as_u64())
        .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
        .unwrap_or(0);

    (is_open, failure_count)
}

/// Extract `(total_executions, total_errors)` from the `(u64, u64)` returned
/// by `router-execution`'s `stats()` view function.
fn extract_execution_stats(val: &serde_json::Value) -> Result<(u64, u64)> {
    let retval = val
        .get("results")
        .and_then(|r| r.get(0))
        .and_then(|r| r.get("retval"))
        .unwrap_or(val);

    // A tuple is an `ScVal::Vec`, decoded either as a bare array or as `{"vec": [...]}`.
    let items = retval
        .as_array()
        .or_else(|| retval.get("vec").and_then(|v| v.as_array()))
        .filter(|items| items.len() == 2)
        .ok_or_else(|| anyhow::anyhow!("could not find (u64, u64) in stats result: {val}"))?;

    let as_u64 =
        |v: &serde_json::Value| v.as_u64().or_else(|| v.get("u64").and_then(|n| n.as_u64()));
    match (as_u64(&items[0]), as_u64(&items[1])) {
        (Some(executions), Some(errors)) => Ok((executions, errors)),
        _ => Err(anyhow::anyhow!(
            "could not find (u64, u64) in stats result: {val}"
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::MockRpcClient;
    use prometheus::Registry;
    use serde_json::json;

    fn make_collector(
        core: &str,
        middleware: &str,
        registry_id: &str,
    ) -> (Collector, RouterMetrics) {
        make_collector_full(core, middleware, registry_id, "", "")
    }

    fn make_collector_full(
        core: &str,
        middleware: &str,
        registry_id: &str,
        quote_id: &str,
        execution_id: &str,
    ) -> (Collector, RouterMetrics) {
        let reg = Registry::new();
        let metrics = RouterMetrics::new(&reg).unwrap();
        let args = Args {
            rpc_url: String::new(),
            core_contract_id: core.to_string(),
            middleware_contract_id: middleware.to_string(),
            registry_contract_id: registry_id.to_string(),
            quote_contract_id: quote_id.to_string(),
            execution_contract_id: execution_id.to_string(),
            scrape_interval_secs: 15,
            listen: "0.0.0.0:9090".to_string(),
            rpc_timeout_secs: 10,
            scrape_concurrency: 8,
        };
        let collector = Collector::new(args, metrics.clone());
        (collector, metrics)
    }

    #[tokio::test]
    async fn test_scrape_core_updates_metrics() {
        let (collector, metrics) = make_collector("CORE_ID", "", "");

        let mock = MockRpcClient::new()
            .with_u64("CORE_ID", "total_routed", 42)
            .with_string_vec("CORE_ID", "get_all_routes", vec![]);

        let ok = collector.scrape_all(&mock).await;
        assert!(ok);

        let val = metrics
            .core_total_routed
            .with_label_values(&["CORE_ID"])
            .get();
        assert_eq!(val, 42.0);
    }

    #[tokio::test]
    async fn test_scrape_core_updates_pause_metric_when_is_paused_view_returns_true() {
        let (collector, metrics) = make_collector("CORE_ID", "", "");

        let mock = MockRpcClient::new()
            .with_u64("CORE_ID", "total_routed", 42)
            .with_bool("CORE_ID", "is_paused", true)
            .with_string_vec("CORE_ID", "get_all_routes", vec![]);

        let ok = collector.scrape_all(&mock).await;
        assert!(ok);

        let val = metrics.core_paused.with_label_values(&["CORE_ID"]).get();
        assert_eq!(val, 1.0);
    }

    #[tokio::test]
    async fn test_scrape_middleware_updates_metrics() {
        let (collector, metrics) = make_collector("", "MW_ID", "");

        let mock = MockRpcClient::new()
            .with_u64("MW_ID", "total_calls", 7)
            .with_string_vec("MW_ID", "get_configured_routes", vec![]);

        let ok = collector.scrape_all(&mock).await;
        assert!(ok);

        let val = metrics
            .middleware_total_calls
            .with_label_values(&["MW_ID"])
            .get();
        assert_eq!(val, 7.0);
    }

    #[tokio::test]
    async fn test_scrape_registry_updates_metrics() {
        let (collector, metrics) = make_collector("", "", "REG_ID");

        let mock = MockRpcClient::new().with_string_vec(
            "REG_ID",
            "get_all_names",
            vec!["oracle".to_string(), "vault".to_string()],
        );

        let ok = collector.scrape_all(&mock).await;
        assert!(ok);

        let val = metrics
            .registry_total_names
            .with_label_values(&["REG_ID"])
            .get();
        assert_eq!(val, 2.0);
    }

    #[tokio::test]
    async fn test_scrape_failure_returns_false_and_increments_error_counter() {
        let (collector, metrics) = make_collector("CORE_ID", "", "");

        // Mock returns no response → scrape_core will fail
        let mock = MockRpcClient::new();

        let ok = collector.scrape_all(&mock).await;
        assert!(!ok);

        let errors = metrics
            .scrape_errors_total
            .with_label_values(&["CORE_ID"])
            .get();
        assert_eq!(errors, 1.0);
    }

    #[tokio::test]
    async fn test_scrape_core_with_routes_and_circuit_breaker() {
        let (collector, metrics) = make_collector("CORE_ID", "MW_ID", "");

        let mock = MockRpcClient::new()
            .with_u64("CORE_ID", "total_routed", 100)
            .with_string_vec("CORE_ID", "get_all_routes", vec!["oracle".to_string()])
            .with_simulate(
                "CORE_ID",
                "get_route",
                json!({ "results": [{ "retval": { "paused": false } }] }),
            )
            .with_u64("MW_ID", "total_calls", 50)
            .with_string_vec("MW_ID", "get_configured_routes", vec!["oracle".to_string()])
            .with_simulate(
                "MW_ID",
                "circuit_breaker_state",
                json!({
                    "results": [{
                        "retval": {
                            "some": { "is_open": true, "failure_count": 3, "opened_at": 1000 }
                        }
                    }]
                }),
            );

        let ok = collector.scrape_all(&mock).await;
        assert!(ok);

        assert_eq!(
            metrics
                .core_total_routed
                .with_label_values(&["CORE_ID"])
                .get(),
            100.0
        );
        assert_eq!(
            metrics
                .middleware_circuit_open
                .with_label_values(&["MW_ID", "oracle"])
                .get(),
            1.0
        );
        assert_eq!(
            metrics
                .middleware_failure_count
                .with_label_values(&["MW_ID", "oracle"])
                .get(),
            3.0
        );
    }

    #[tokio::test]
    async fn test_scrape_quote_counts_events() {
        use crate::rpc::ContractEvent;
        let (collector, metrics) = make_collector_full("", "", "", "QUOTE_ID", "");

        let make_event = |topic: &str, id: &str| ContractEvent {
            contract_id: "QUOTE_ID".to_string(),
            id: id.to_string(),
            topic: vec![serde_json::json!(topic)],
            value: serde_json::json!({}),
        };

        let mock = MockRpcClient::new()
            .with_events(
                "QUOTE_ID",
                "quote_calculated",
                vec![
                    make_event("quote_calculated", "1"),
                    make_event("quote_calculated", "2"),
                ],
            )
            .with_events(
                "QUOTE_ID",
                "fee_estimated",
                vec![make_event("fee_estimated", "3")],
            );

        let ok = collector.scrape_all(&mock).await;
        assert!(ok);

        assert_eq!(
            metrics
                .quote_total_generated
                .with_label_values(&["QUOTE_ID"])
                .get(),
            2.0
        );
        assert_eq!(
            metrics
                .quote_total_fee_estimated
                .with_label_values(&["QUOTE_ID"])
                .get(),
            1.0
        );
    }

    #[tokio::test]
    async fn test_scrape_quote_accumulates_across_cycles() {
        use crate::rpc::ContractEvent;
        let (collector, metrics) = make_collector_full("", "", "", "QUOTE_ID", "");

        let make_event = |topic: &str, id: &str| ContractEvent {
            contract_id: "QUOTE_ID".to_string(),
            id: id.to_string(),
            topic: vec![serde_json::json!(topic)],
            value: serde_json::json!({}),
        };

        // Two pages/cycles of events
        let mock = MockRpcClient::new()
            .with_event_pages(
                "QUOTE_ID",
                "quote_calculated",
                vec![
                    vec![make_event("quote_calculated", "1")],
                    vec![make_event("quote_calculated", "2"), make_event("quote_calculated", "3")],
                ],
            )
            .with_event_pages(
                "QUOTE_ID",
                "fee_estimated",
                vec![
                    vec![make_event("fee_estimated", "10")],
                    vec![make_event("fee_estimated", "11")],
                ],
            );

        // Cycle 1
        let ok1 = collector.scrape_all(&mock).await;
        assert!(ok1);

        assert_eq!(
            metrics
                .quote_total_generated
                .with_label_values(&["QUOTE_ID"])
                .get(),
            1.0
        );
        assert_eq!(
            metrics
                .quote_total_fee_estimated
                .with_label_values(&["QUOTE_ID"])
                .get(),
            1.0
        );

        // Cycle 2
        let ok2 = collector.scrape_all(&mock).await;
        assert!(ok2);

        assert_eq!(
            metrics
                .quote_total_generated
                .with_label_values(&["QUOTE_ID"])
                .get(),
            3.0 // 1 + 2
        );
        assert_eq!(
            metrics
                .quote_total_fee_estimated
                .with_label_values(&["QUOTE_ID"])
                .get(),
            2.0 // 1 + 1
        );
    }

    #[tokio::test]
    async fn test_scrape_execution_reads_view_functions() {
        let (collector, metrics) = make_collector_full("", "", "", "", "EXEC_ID");

        let mock = MockRpcClient::new()
            .with_simulate(
                "EXEC_ID",
                "stats",
                json!({ "results": [{ "retval": [{ "u64": 42 }, { "u64": 5 }] }] }),
            )
            .with_u64("EXEC_ID", "max_retries", 3);

        let ok = collector.scrape_all(&mock).await;
        assert!(ok);

        assert_eq!(
            metrics
                .execution_total_executions
                .with_label_values(&["EXEC_ID"])
                .get(),
            42.0
        );
        assert_eq!(
            metrics
                .execution_total_errors
                .with_label_values(&["EXEC_ID"])
                .get(),
            5.0
        );
        assert_eq!(
            metrics
                .execution_max_retries
                .with_label_values(&["EXEC_ID"])
                .get(),
            3.0
        );
    }

    #[tokio::test]
    async fn test_scrape_execution_fails_when_values_are_missing() {
        let (collector, metrics) = make_collector_full("", "", "", "", "EXEC_ID");

        // `stats` returns something that is not a (u64, u64) tuple and
        // `max_retries` has no response at all.
        let mock = MockRpcClient::new().with_simulate(
            "EXEC_ID",
            "stats",
            json!({ "results": [{ "retval": null }] }),
        );

        let ok = collector.scrape_all(&mock).await;
        assert!(!ok);
        assert_eq!(
            metrics
                .scrape_errors_total
                .with_label_values(&["EXEC_ID"])
                .get(),
            1.0
        );
    }

    #[test]
    fn test_extract_execution_stats_formats() {
        let bare = json!({ "results": [{ "retval": [7, 2] }] });
        assert_eq!(extract_execution_stats(&bare).unwrap(), (7, 2));

        let vec = json!({ "results": [{ "retval": { "vec": [{ "u64": 9 }, { "u64": 1 }] } }] });
        assert_eq!(extract_execution_stats(&vec).unwrap(), (9, 1));
    }

    #[test]
    fn test_extract_execution_stats_rejects_missing_values() {
        for retval in [json!(null), json!([1]), json!(["a", "b"])] {
            let val = json!({ "results": [{ "retval": retval }] });
            assert!(extract_execution_stats(&val).is_err());
        }
    }

    #[test]
    fn test_extract_route_paused_true() {
        let val = json!({
            "results": [{ "retval": { "paused": true } }]
        });
        assert_eq!(extract_route_paused(&val), Some(true));
    }

    #[test]
    fn test_extract_route_paused_false() {
        let val = json!({ "paused": false });
        assert_eq!(extract_route_paused(&val), Some(false));
    }

    #[test]
    fn test_extract_circuit_breaker_open() {
        let val = json!({
            "results": [{
                "retval": {
                    "some": {
                        "is_open": true,
                        "failure_count": 5,
                        "opened_at": 1000
                    }
                }
            }]
        });
        assert_eq!(extract_circuit_breaker_state(&val), (true, 5));
    }

    #[test]
    fn test_extract_circuit_breaker_none() {
        let val = json!({
            "results": [{ "retval": null }]
        });
        assert_eq!(extract_circuit_breaker_state(&val), (false, 0));
    }

    #[test]
    fn test_extract_circuit_breaker_closed() {
        let val = json!({
            "results": [{
                "retval": {
                    "some": {
                        "is_open": false,
                        "failure_count": 2,
                        "opened_at": 0
                    }
                }
            }]
        });
        assert_eq!(extract_circuit_breaker_state(&val), (false, 2));
    }

    #[test]
    fn test_extract_circuit_breaker_failure_count_saturates() {
        let val = json!({
            "results": [{
                "retval": {
                    "some": {
                        "is_open": true,
                        "failure_count": u64::from(u32::MAX) + 1,
                        "opened_at": 0
                    }
                }
            }]
        });
        assert_eq!(extract_circuit_breaker_state(&val), (true, u32::MAX));
    }
}
