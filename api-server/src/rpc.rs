/// Soroban RPC client for simulation, fee estimation, and contract reads.
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

use crate::types::{RouteEntryResponse, RouteMetadataResponse};

// Fee estimation constants
const BASE_FEE: i64 = 100;
const SURGE_THRESHOLD_BPS: u32 = 8_000;
const SURGE_MULTIPLIER: u32 = 200;
const NORMAL_MULTIPLIER: u32 = 100;

/// Applies the surge multiplier to compute `total_fee`.
///
/// Uses saturating arithmetic (Issue #1606): `resource_fee` is parsed from
/// the RPC's `minResourceFee` string or derived from the client-controlled
/// `amount`, so `(base_fee + resource_fee) * multiplier` can overflow `i64`
/// — a panic in debug builds and a negative fee in release builds.
fn total_fee_with_surge(base_fee: i64, resource_fee: i64, surge_multiplier: u32) -> i64 {
    base_fee
        .saturating_add(resource_fee)
        .saturating_mul(surge_multiplier as i64)
        / 100
}

#[derive(Debug, Clone)]
pub struct SorobanRpcClient {
    pub rpc_url: String,
    pub router_core_contract_id: Option<String>,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'a str,
    id: u64,
    method: &'a str,
    params: serde_json::Value,
}

#[derive(Deserialize, Debug)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize, Debug)]
struct JsonRpcError {
    message: String,
}

#[derive(Deserialize, Debug)]
pub struct SimulateTransactionResult {
    #[serde(rename = "minResourceFee", default)]
    pub min_resource_fee: String,
    pub error: Option<String>,
    /// Diagnostic/contract events returned by the RPC node. Surfaced to API
    /// callers as `SimulationDetail.diagnostics`.
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
struct SimulateTransactionResultWithReturnValue {
    #[allow(dead_code)]
    #[serde(rename = "minResourceFee", default)]
    pub min_resource_fee: String,
    pub error: Option<String>,
    #[serde(default)]
    pub results: Vec<InvokeResult>,
}

#[derive(Deserialize, Debug)]
struct InvokeResult {
    pub xdr: String,
}

#[derive(Debug)]
pub struct FeeBreakdown {
    pub base_fee: i64,
    pub resource_fee: i64,
    pub total_fee: i64,
    pub surge_multiplier: u32,
    pub high_load: bool,
    pub would_succeed: bool,
    pub fee_estimated: bool,
    /// `true` only when fees were derived from a live RPC simulation.
    /// `false` when the heuristic fallback was used (RPC unreachable).
    #[allow(dead_code)]
    pub simulated: bool,
    /// Diagnostic events from the RPC simulation, formatted as strings.
    /// Empty when the heuristic fallback was used.
    pub diagnostics: Vec<String>,
}

impl SorobanRpcClient {
    pub fn with_timeout(
        rpc_url: impl Into<String>,
        router_core_contract_id: Option<String>,
        rpc_timeout_secs: u64,
    ) -> Self {
        let rpc_timeout = std::time::Duration::from_secs(rpc_timeout_secs);
        let connect_timeout = std::time::Duration::from_secs(5);

        Self {
            rpc_url: rpc_url.into(),
            router_core_contract_id,
            http: reqwest::Client::builder()
                .connect_timeout(connect_timeout)
                .timeout(rpc_timeout)
                .build()
                .expect("failed to build HTTP client"),
        }
    }

    /// Ping the RPC node via `getLatestLedger`. Returns `Ok(())` if reachable.
    pub async fn health_check(&self) -> Result<()> {
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "getLatestLedger",
            params: serde_json::json!({}),
        };
        let resp = self
            .http
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await
            .map_err(|e| anyhow!("RPC unreachable: {e}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("RPC returned status {}", resp.status()));
        }
        Ok(())
    }

    pub async fn simulate(
        &self,
        target: &str,
        function: &str,
        amount: i64,
        network_load_bps: u32,
    ) -> Result<FeeBreakdown> {
        match self.call_simulate_rpc(target, function).await {
            Ok(result) => {
                let would_succeed = result.error.is_none();
                let (resource_fee, fee_estimated) = match result.min_resource_fee.parse::<i64>() {
                    // Clamp negatives: a bogus negative minResourceFee from
                    // the RPC must not yield a negative total (Issue #1606).
                    Ok(v) => (v.max(0), true),
                    Err(_) => {
                        warn!(
                            raw = %result.min_resource_fee,
                            "failed to parse min_resource_fee from RPC response; using fallback 1000 stroops"
                        );
                        (1_000, false)
                    }
                };
                let base_fee = BASE_FEE;
                let (surge_multiplier, high_load) = if network_load_bps >= SURGE_THRESHOLD_BPS {
                    (SURGE_MULTIPLIER, true)
                } else {
                    (NORMAL_MULTIPLIER, false)
                };
                let total_fee = total_fee_with_surge(base_fee, resource_fee, surge_multiplier);
                let diagnostics = Self::format_events(&result.events);
                Ok(FeeBreakdown {
                    base_fee,
                    resource_fee,
                    total_fee,
                    surge_multiplier,
                    high_load,
                    would_succeed,
                    fee_estimated,
                    simulated: true,
                    diagnostics,
                })
            }
            Err(_) => Ok(Self::heuristic_estimate(amount, network_load_bps)),
        }
    }

    pub async fn get_all_routes(&self, contract_id: &str) -> Result<Vec<String>> {
        let placeholder_xdr = format!("AAAAAgAAAAEAAAAA{contract_id}get_all_routesAAAAAAAAAAAA=");

        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "simulateTransaction",
            params: serde_json::json!({ "transaction": placeholder_xdr }),
        };

        let resp: JsonRpcResponse<SimulateTransactionResultWithReturnValue> = self
            .http
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await
            .map_err(|e| anyhow!("RPC request failed: {e}"))?
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse RPC response: {e}"))?;

        if let Some(err) = resp.error {
            return Err(anyhow!("RPC error: {}", err.message));
        }

        let result = resp.result.ok_or_else(|| anyhow!("empty RPC result"))?;

        if let Some(err) = result.error {
            return Err(anyhow!("contract error: {err}"));
        }

        let routes = result
            .results
            .into_iter()
            .next()
            .and_then(|r| Self::decode_string_vec_xdr(&r.xdr))
            .unwrap_or_default();

        Ok(routes)
    }

    pub async fn get_route(&self, name: &str) -> Result<Option<RouteEntryResponse>> {
        let contract_id = self
            .router_core_contract_id
            .as_deref()
            .ok_or_else(|| anyhow!("ROUTER_CORE_CONTRACT_ID not configured"))?;

        let placeholder_xdr = format!("get_route:{contract_id}:{name}");

        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "simulateTransaction",
            params: serde_json::json!({
                "transaction": placeholder_xdr,
                "resourceConfig": { "instructionLeeway": 3000000 }
            }),
        };

        let resp: JsonRpcResponse<Value> = self
            .http
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;

        if let Some(err) = resp.error {
            return Err(anyhow!("RPC error: {}", err.message));
        }

        let result = match resp.result {
            Some(r) => r,
            None => return Ok(None),
        };

        Self::parse_route_entry_from_rpc(&result)
    }

    /// Parses the `simulateTransaction` result of a `router-core::get_route`
    /// call.
    ///
    /// `get_route` returns `Option<RouteEntry>`, so a missing route comes back
    /// as an empty/void result, never as a simulation error. Any `error` field
    /// therefore means a real failure (contract panic, bad argument, auth
    /// failure, ...) and is returned as `Err` so the handler answers 500
    /// instead of a misleading 404.
    fn parse_route_entry_from_rpc(result: &Value) -> Result<Option<RouteEntryResponse>> {
        if let Some(err) = result.get("error").filter(|e| !e.is_null()) {
            let msg = err
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| err.to_string());
            return Err(anyhow!("contract simulation error: {msg}"));
        }

        let results = match result.get("results").and_then(|r| r.as_array()) {
            Some(r) if !r.is_empty() => r,
            _ => return Ok(None),
        };

        let entry_json = results[0].get("xdr").cloned().unwrap_or(Value::Null);
        if entry_json.is_null() {
            return Ok(None);
        }

        let map = match entry_json.get("map").and_then(|m| m.as_array()) {
            Some(m) => m.clone(),
            None => return Ok(None),
        };

        let mut address = String::new();
        let mut route_name = String::new();
        let mut paused = false;
        let mut updated_by = String::new();
        let mut metadata: Option<RouteMetadataResponse> = None;

        for item in &map {
            let key = item
                .get("key")
                .and_then(|k| k.get("sym"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            let val = &item["val"];
            match key {
                "address" => {
                    address = val
                        .get("address")
                        .and_then(|a| a.as_str())
                        .unwrap_or("")
                        .to_string();
                }
                "name" => {
                    route_name = val
                        .get("str")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                }
                "paused" => {
                    paused = val.get("b").and_then(|b| b.as_bool()).unwrap_or(false);
                }
                "updated_by" => {
                    updated_by = val
                        .get("address")
                        .and_then(|a| a.as_str())
                        .unwrap_or("")
                        .to_string();
                }
                "metadata" => {
                    if let Some(meta_map) = val.get("map").and_then(|m| m.as_array()) {
                        let mut description = String::new();
                        let mut tags = Vec::new();
                        let mut owner = String::new();
                        for meta_item in meta_map {
                            let mk = meta_item
                                .get("key")
                                .and_then(|k| k.get("sym"))
                                .and_then(|s| s.as_str())
                                .unwrap_or("");
                            let mv = &meta_item["val"];
                            match mk {
                                "description" => {
                                    description = mv
                                        .get("str")
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                }
                                "owner" => {
                                    owner = mv
                                        .get("address")
                                        .and_then(|a| a.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                }
                                "tags" => {
                                    if let Some(tag_vec) = mv.get("vec").and_then(|v| v.as_array())
                                    {
                                        tags = tag_vec
                                            .iter()
                                            .filter_map(|t| {
                                                t.get("str")
                                                    .and_then(|s| s.as_str())
                                                    .map(|s| s.to_string())
                                            })
                                            .collect();
                                    }
                                }
                                _ => {}
                            }
                        }
                        metadata = Some(RouteMetadataResponse {
                            description,
                            tags,
                            owner,
                        });
                    }
                }
                _ => {}
            }
        }

        if address.is_empty() {
            return Ok(None);
        }

        Ok(Some(RouteEntryResponse {
            address,
            name: route_name,
            paused,
            updated_by,
            metadata,
        }))
    }

    async fn call_simulate_rpc(
        &self,
        target: &str,
        function: &str,
    ) -> Result<SimulateTransactionResult> {
        let placeholder_xdr = format!("AAAAAgAAAAEAAAAA{target}{function}AAAAAAAAAAA=");
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "simulateTransaction",
            params: serde_json::json!({ "transaction": placeholder_xdr }),
        };
        let resp: JsonRpcResponse<SimulateTransactionResult> = self
            .http
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await?
            .json()
            .await?;
        if let Some(err) = resp.error {
            return Err(anyhow!("RPC error: {}", err.message));
        }
        resp.result.ok_or_else(|| anyhow!("empty RPC result"))
    }

    /// Decodes a minimal XDR-encoded `Vec<String>`:
    /// 4-byte big-endian element count, then for each element:
    /// 4-byte big-endian byte length + UTF-8 bytes padded to the next 4-byte boundary.
    fn decode_string_vec_xdr(xdr: &str) -> Option<Vec<String>> {
        let bytes = base64_decode(xdr)?;
        if bytes.len() < 4 {
            return Some(Vec::new());
        }
        let count = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let mut result = Vec::with_capacity(count);
        let mut pos = 4;
        for _ in 0..count {
            if pos + 4 > bytes.len() {
                break;
            }
            let len =
                u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
                    as usize;
            pos += 4;
            if pos + len > bytes.len() {
                break;
            }
            if let Ok(s) = std::str::from_utf8(&bytes[pos..pos + len]) {
                result.push(s.trim_end_matches('\0').to_string());
            }
            // Advance past padding to the next 4-byte boundary.
            pos += (len + 3) & !3;
        }
        Some(result)
    }

    fn heuristic_estimate(amount: i64, network_load_bps: u32) -> FeeBreakdown {
        let base_fee = BASE_FEE;
        let resource_fee: i64 = {
            let scaled = amount / 1_000;
            if scaled < 100 {
                100
            } else {
                scaled
            }
        };
        let (surge_multiplier, high_load) = if network_load_bps >= SURGE_THRESHOLD_BPS {
            (SURGE_MULTIPLIER, true)
        } else {
            (NORMAL_MULTIPLIER, false)
        };
        let total_fee = total_fee_with_surge(base_fee, resource_fee, surge_multiplier);
        FeeBreakdown {
            base_fee,
            resource_fee,
            total_fee,
            surge_multiplier,
            high_load,
            // No simulation was performed — cannot assert the transaction will succeed.
            would_succeed: false,
            // Fees are a heuristic guess, not an RPC-derived measurement.
            fee_estimated: false,
            simulated: false,
            diagnostics: Vec::new(),
        }
    }

    /// Formats raw RPC events as strings. Events are usually base64 XDR
    /// strings and are passed through as-is; anything else is JSON-encoded.
    fn format_events(events: &[Value]) -> Vec<String> {
        events
            .iter()
            .map(|e| match e.as_str() {
                Some(s) => s.to_string(),
                None => e.to_string(),
            })
            .collect()
    }
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: [u8; 128] = {
        let mut t = [255u8; 128];
        let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut i = 0usize;
        while i < 64 {
            t[chars[i] as usize] = i as u8;
            i += 1;
        }
        t
    };

    let input = input.trim_end_matches('=');
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let bytes = input.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let b0 = *TABLE.get(bytes[i] as usize)?;
        let b1 = *TABLE.get(bytes[i + 1] as usize)?;
        let b2 = *TABLE.get(bytes[i + 2] as usize)?;
        let b3 = *TABLE.get(bytes[i + 3] as usize)?;
        if b0 == 255 || b1 == 255 || b2 == 255 || b3 == 255 {
            return None;
        }
        out.push((b0 << 2) | (b1 >> 4));
        out.push((b1 << 4) | (b2 >> 2));
        out.push((b2 << 6) | b3);
        i += 4;
    }
    match bytes.len() - i {
        2 => {
            let b0 = *TABLE.get(bytes[i] as usize)?;
            let b1 = *TABLE.get(bytes[i + 1] as usize)?;
            if b0 == 255 || b1 == 255 {
                return None;
            }
            out.push((b0 << 2) | (b1 >> 4));
        }
        3 => {
            let b0 = *TABLE.get(bytes[i] as usize)?;
            let b1 = *TABLE.get(bytes[i + 1] as usize)?;
            let b2 = *TABLE.get(bytes[i + 2] as usize)?;
            if b0 == 255 || b1 == 255 || b2 == 255 {
                return None;
            }
            out.push((b0 << 2) | (b1 >> 4));
            out.push((b1 << 4) | (b2 >> 2));
        }
        0 => {}
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_estimate_does_not_assert_success() {
        let result = SorobanRpcClient::heuristic_estimate(1_000_000, 5_000);
        assert!(
            !result.would_succeed,
            "heuristic path must not claim the transaction will succeed"
        );
        assert!(
            !result.simulated,
            "heuristic path must report simulated = false"
        );
        assert!(
            !result.fee_estimated,
            "heuristic path must report fee_estimated = false"
        );
    }

    #[test]
    fn heuristic_estimate_high_load_surge() {
        let result = SorobanRpcClient::heuristic_estimate(500_000, 9_000);
        assert!(result.high_load);
        assert_eq!(result.surge_multiplier, 200);
        assert!(!result.would_succeed);
        assert!(!result.simulated);
    }

    #[test]
    fn heuristic_estimate_minimum_resource_fee() {
        // amount / 1_000 = 0, which is below the 100 floor
        let result = SorobanRpcClient::heuristic_estimate(50, 0);
        assert_eq!(result.resource_fee, 100);
    }

    #[test]
    fn total_fee_with_surge_saturates_instead_of_overflowing() {
        // Issue #1606: (base_fee + i64::MAX) * 200 overflows i64. It must
        // saturate to a non-negative value, not panic (debug) or wrap
        // negative (release).
        let total = total_fee_with_surge(BASE_FEE, i64::MAX, SURGE_MULTIPLIER);
        assert!(
            total >= 0,
            "saturated total_fee must not be negative, got {total}"
        );
    }

    #[test]
    fn heuristic_estimate_i64_max_amount_at_surge_does_not_panic() {
        // Issue #1606 acceptance: amount = i64::MAX with network_load_bps =
        // 10000 (surge) must return a saturated, non-negative fee.
        let result = SorobanRpcClient::heuristic_estimate(i64::MAX, 10_000);
        assert!(result.high_load);
        assert_eq!(result.surge_multiplier, SURGE_MULTIPLIER);
        assert!(
            result.total_fee >= 0,
            "total_fee must not be negative, got {}",
            result.total_fee
        );
        assert!(result.resource_fee >= 0);
    }

    #[test]
    fn decode_string_vec_xdr_empty_input() {
        // Less than 4 bytes → empty vec, not None
        assert_eq!(
            SorobanRpcClient::decode_string_vec_xdr(""),
            Some(Vec::<String>::new())
        );
    }
}
