# Technical Overview: Fix `SimulateRequest.fee_bps` Unused Parameter Bug

## Root Cause Analysis
In `api-server/src/types.rs`, the `SimulateRequest` struct defines a `fee_bps` parameter (basis points, defaulting to `30` via `default_fee_bps()`). While deserialization, validation, and test payloads set this value, the handler (`handlers::simulate` in `api-server/src/handlers.rs`) omitted passing `req.fee_bps` into the simulation engine (`Simulator` / `SimulationConfig`). As a result, simulations ignored user-configured fee settings and operated using unconfigurable defaults or zero-fee assumptions.

## Solution Strategy
1. **Forward `fee_bps` in `handlers::simulate`**: Pass `req.fee_bps` directly into the simulation context or configuration struct before executing the swap/transaction simulation.
2. **Apply Fee Logic**: Ensure the simulator's swap calculation applies the basis point fee (`fee_amount = amount * fee_bps / 10_000`) when calculating net output amounts or gas/fee breakdowns.
3. **Add Integration/Unit Test**: Update tests in `api-server/src/tests.rs` to verify that varying `fee_bps` values produce appropriately adjusted simulation results.

---

# Code Fix (Rust Diff)

### `api-server/src/handlers.rs`

```rust
pub async fn simulate(
    State(state): State<AppState>,
    Json(req): Json<SimulateRequest>,
) -> Result<Json<SimulateResponse>, ApiError> {
    req.validate()?;

    // Construct simulation configuration using request parameters including fee_bps
    let sim_config = SimulationConfig {
        amount_in: req.amount_in,
        token_in: req.token_in,
        token_out: req.token_out,
        fee_bps: req.fee_bps, // Pass the validated fee_bps parameter
        slippage_tolerance: req.slippage_tolerance,
    };

    // Run simulation with updated config
    let sim_result = state.simulator.simulate(&sim_config).await?;

    Ok(Json(SimulateResponse {
        amount_out: sim_result.amount_out,
        fee_amount: sim_result.fee_amount,
        fee_bps: req.fee_bps,
        execution_price: sim_result.execution_price,
        price_impact: sim_result.price_impact,
    }))
}
```

---

### `api-server/src/tests.rs`

```rust
#[tokio::test]
async fn test_simulate_request_custom_fee_bps() {
    let app = create_test_app().await;

    // Test with standard default fee (30 bps)
    let req_default = SimulateRequest {
        amount_in: 1_000_000,
        token_in: "USDC".to_string(),
        token_out: "ETH".to_string(),
        fee_bps: 30,
        ..Default::default()
    };

    let response_default = app.simulate(req_default).await;
    assert_eq!(response_default.fee_bps, 30);

    // Test with custom fee (50 bps)
    let req_custom = SimulateRequest {
        amount_in: 1_000_000,
        token_in: "USDC".to_string(),
        token_out: "ETH".to_string(),
        fee_bps: 50,
        ..Default::default()
    };

    let response_custom = app.simulate(req_custom).await;
    assert_eq!(response_custom.fee_bps, 50);
    assert!(
        response_custom.amount_out < response_default.amount_out,
        "Higher fee_bps should result in lower amount_out"
    );
}
```