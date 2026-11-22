// Issues #1604, #1605, #1606, #1607 - API Server Fixes

// Issue #1607: Fix rate limiter documentation
// RateLimitConfig doc BEFORE: "Rate-limit configuration for /simulate requests."
// AFTER: "Rate-limit applied to all routes router-wide"
// --rate-limit-window-secs BEFORE: "Sliding window for rate limiting"
// AFTER: "Fixed window for rate limiting"

// Issue #1606: Prevent total_fee overflow
pub fn calculate_total_fee(base_fee: i64, resource_fee: i64, surge_multiplier: u32) -> Result<i64> {
    // FIX: Use checked_mul to prevent overflow
    let resource_with_surge = resource_fee
        .checked_mul(surge_multiplier as i64)
        .ok_or(Error::FeeOverflow)?
        .checked_div(100)
        .ok_or(Error::FeeOverflow)?;

    let total = base_fee
        .checked_add(resource_with_surge)
        .ok_or(Error::FeeOverflow)?;

    Ok(total)
}

// Issue #1605: Validate RPC health check response
pub fn health_check(env: &Env, client: &SorobanRpcClient) -> Result<()> {
    // FIX: Check JSON-RPC response body, not just HTTP status
    let resp = client.get_latest_ledger().await?;

    // Validate that response contains actual data, not error
    if resp.error.is_some() {
        return Err(Error::RpcError(format!("RPC error: {:?}", resp.error)));
    }

    if resp.result.is_none() {
        return Err(Error::RpcError("No result in RPC response".to_string()));
    }

    Ok(())
}

// Issue #1604: Extract JSON-RPC boilerplate
pub mod json_rpc {
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    pub struct JsonRpcRequest<T> {
        pub jsonrpc: String,
        pub id: String,
        pub method: String,
        pub params: T,
    }

    #[derive(Deserialize)]
    pub struct JsonRpcResponse<T> {
        pub result: Option<T>,
        pub error: Option<JsonRpcError>,
    }

    #[derive(Deserialize)]
    pub struct JsonRpcError {
        pub code: i32,
        pub message: String,
    }

    // FIX: Shared helper to POST and parse JSON-RPC
    pub async fn call_rpc<Req: Serialize, Resp: for<'de> Deserialize<'de>>(
        client: &reqwest::Client,
        url: &str,
        method: &str,
        params: Req,
    ) -> Result<Resp> {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: uuid::Uuid::new_v4().to_string(),
            method: method.to_string(),
            params,
        };

        let resp = client
            .post(url)
            .json(&req)
            .send()
            .await
            .map_err(|e| Error::RpcRequest(format!("RPC request failed: {e}")))?;

        let json_resp: JsonRpcResponse<Resp> = resp
            .json()
            .await
            .map_err(|e| Error::RpcParse(format!("Failed to parse RPC response: {e}")))?;

        // FIX: Consistent error handling
        if let Some(err) = json_resp.error {
            return Err(Error::RpcError(format!(
                "RPC error {}: {}",
                err.code, err.message
            )));
        }

        json_resp
            .result
            .ok_or_else(|| Error::RpcError("Missing result in RPC response".to_string()))
    }
}
