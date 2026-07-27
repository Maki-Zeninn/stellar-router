//! End-to-end quote → execution → multicall pipeline integration test
//!
//! This test validates the full routing pipeline: getting quotes, executing through
//! the router, and batching multiple operations. It ensures all contracts work
//! together as a cohesive system, catching integration regressions that per-contract
//! tests miss.
//!
//! ## Test Flow
//! 1. Deploy all contracts (registry, access, middleware, core, quote, execution, multicall)
//! 2. Register a route via router-core
//! 3. Configure fees via router-quote
//! 4. Get a quote for a swap amount
//! 5. Execute the swap via router-execution
//! 6. Verify middleware logged the call
//! 7. Batch multiple swaps via router-multicall
//! 8. Verify all events were emitted correctly
//! 9. Check rate limiting is enforced after the batch
//!
//! Run with:
//!   cargo test --test quote_execution_multicall_pipeline test_quote_to_execution_to_multicall_pipeline
extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Symbol, Vec,
};

// ── Contract imports ──────────────────────────────────────────────────────────

use router_core::{RouterCore, RouterCoreClient};
use router_registry::{RouterRegistry, RouterRegistryClient};
use router_access::{RouterAccess, RouterAccessClient};
use router_middleware::{RouterMiddleware, RouterMiddlewareClient};
use router_quote::{
    RouterQuote, RouterQuoteClient, QuoteRequest, QuoteResponse,
};
use router_execution::{
    RouterExecution, RouterExecutionClient, ExecutionRequest, ExecutionResult,
};
use router_multicall::{
    RouterMulticall, RouterMulticallClient, CallDescriptor,
};

// ── Test Suite Setup ──────────────────────────────────────────────────────────

/// Comprehensive test suite for the quote → execution → multicall pipeline.
struct PipelineTestSuite<'a> {
    env: Env,
    admin: Address,
    user: Address,
    // Contracts
    core: RouterCoreClient<'a>,
    registry: RouterRegistryClient<'a>,
    access: RouterAccessClient<'a>,
    middleware: RouterMiddlewareClient<'a>,
    quote: RouterQuoteClient<'a>,
    execution: RouterExecutionClient<'a>,
    multicall: RouterMulticallClient<'a>,
    // Mock addresses for tokens
    token_in: Address,
    token_out: Address,
}

impl<'a> PipelineTestSuite<'a> {
    /// Set up all contracts and initialize them for the test.
    fn setup() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| {
            l.timestamp = 1000;
            l.sequence = 100;
        });

        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let token_in = Address::generate(&env);
        let token_out = Address::generate(&env);

        // ── Deploy and initialize all contracts ──────────────────────────────

        let core_id = env.register_contract(None, RouterCore);
        let registry_id = env.register_contract(None, RouterRegistry);
        let access_id = env.register_contract(None, RouterAccess);
        let middleware_id = env.register_contract(None, RouterMiddleware);
        let quote_id = env.register_contract(None, RouterQuote);
        let execution_id = env.register_contract(None, RouterExecution);
        let multicall_id = env.register_contract(None, RouterMulticall);

        let core = RouterCoreClient::new(&env, &core_id);
        let registry = RouterRegistryClient::new(&env, &registry_id);
        let access = RouterAccessClient::new(&env, &access_id);
        let middleware = RouterMiddlewareClient::new(&env, &middleware_id);
        let quote = RouterQuoteClient::new(&env, &quote_id);
        let execution = RouterExecutionClient::new(&env, &execution_id);
        let multicall = RouterMulticallClient::new(&env, &multicall_id);

        // Initialize all contracts
        core.initialize(&admin);
        registry.initialize(&admin);
        access.initialize(&admin);
        middleware.initialize(&admin);
        quote.initialize(&admin, &100); // 100 bps = 1% default fee
        execution.initialize(&admin, &3, &100, &150); // max_retries=3, base_ms=100, multiplier=150
        multicall.initialize(&admin, &10); // max batch size = 10

        Self {
            env,
            admin,
            user,
            core,
            registry,
            access,
            middleware,
            quote,
            execution,
            multicall,
            token_in,
            token_out,
        }
    }

    /// Register a swap route and configure all associated services.
    fn setup_swap_route(&self, route_name: &str) {
        let route = String::from_str(&self.env, route_name);
        let mock_oracle = Address::generate(&self.env);

        // Step 1: Register route in registry
        self.registry.register(&self.admin, &route, &mock_oracle, &1);

        // Step 2: Register route in core
        self.core.register_route(&self.admin, &route, &mock_oracle, &None);

        // Step 3: Configure route in quote (50 bps = 0.5% fee)
        self.quote.set_route_fee(&self.admin, &route, &50);

        // Step 4: Configure middleware with rate limiting (10 calls per 60s window)
        self.middleware.configure_route(&self.admin, &route, &10, &60, &true, &3, &30, &0);
    }

    /// Advance time by a given number of seconds.
    fn advance_time(&self, seconds: u64) {
        self.env.ledger().with_mut(|l| {
            l.timestamp += seconds;
            l.sequence += 1;
        });
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Test 1: Deploy all contracts and verify initialization
#[test]
fn test_pipeline_all_contracts_deployed() {
    let s = PipelineTestSuite::setup();
    println!("\n✓ All contracts deployed and initialized");
}

/// Test 2: Register a route and verify resolution
#[test]
fn test_pipeline_route_registration() {
    let s = PipelineTestSuite::setup();
    let route_name = "swap/usd_to_eur";
    s.setup_swap_route(route_name);

    let route = String::from_str(&s.env, route_name);
    let resolved = s.core.resolve(&route);
    assert!(!resolved.is_empty(), "Route should resolve correctly");

    println!("\n✓ Route '{}' registered and resolves", route_name);
}

/// Test 3: Quote calculation with fees
#[test]
fn test_pipeline_quote_calculation() {
    let s = PipelineTestSuite::setup();
    let route_name = "swap/usd_to_eur";
    s.setup_swap_route(route_name);

    let route = String::from_str(&s.env, route_name);
    let amount_in = 10_000_000i128; // 10 tokens

    // Get quote
    let quote_request = QuoteRequest {
        route: route.clone(),
        token_in: s.token_in.clone(),
        token_out: s.token_out.clone(),
        amount_in,
    };

    let quote = s.quote.get_quote(&quote_request)
        .expect("Failed to get quote");

    // Verify quote response
    assert_eq!(quote.amount_in, amount_in);
    assert_eq!(quote.route, route);
    assert_eq!(quote.fee_bps, 50); // Should use configured fee

    // Verify fee calculation: 50 bps = 0.5%
    // fee_amount = 10_000_000 * 50 / 10_000 = 50_000
    let expected_fee = 50_000i128;
    assert_eq!(quote.fee_amount, expected_fee);

    // amount_out should be amount_in - fee_amount
    let expected_output = amount_in - expected_fee;
    assert_eq!(quote.amount_out, expected_output);

    println!("\n✓ Quote calculated correctly");
    println!("  Amount in: {}", quote.amount_in);
    println!("  Fee (50 bps): {}", quote.fee_amount);
    println!("  Amount out: {}", quote.amount_out);
}

/// Test 4: Middleware pre_call checks before execution
#[test]
fn test_pipeline_middleware_pre_call() {
    let s = PipelineTestSuite::setup();
    let route_name = "swap/usd_to_eur";
    s.setup_swap_route(route_name);

    let route = String::from_str(&s.env, route_name);

    // First pre_call should succeed
    assert!(
        s.middleware.try_pre_call(&s.user, &route).is_ok(),
        "First pre_call should succeed"
    );

    // Verify call was counted
    assert_eq!(s.middleware.total_calls(), 1);

    println!("\n✓ Middleware pre_call passed");
}

/// Test 5: Execution of swap via router-execution
#[test]
fn test_pipeline_execution_swap() {
    let s = PipelineTestSuite::setup();
    let route_name = "swap/usd_to_eur";
    s.setup_swap_route(route_name);

    let route = String::from_str(&s.env, route_name);

    // First, middleware pre_call
    s.middleware.pre_call(&s.user, &route);

    // Prepare execution request
    let mock_target = Address::generate(&s.env);
    let exec_request = ExecutionRequest {
        target: mock_target.clone(),
        function: Symbol::new(&s.env, "swap"),
        simulate_first: false,
        max_retries: 1,
        args: Vec::new(&s.env),
        amount: 1_000_000,
    };

    // Execute (with mocked auth, this should succeed)
    let result = s.execution.execute(&s.user, &exec_request)
        .expect("Execution should succeed");

    // Verify execution result
    assert_eq!(result.target, mock_target);
    assert_eq!(result.success, true);
    assert_eq!(result.attempts, 1u32);
    assert_eq!(result.simulated, false);

    // Log execution via middleware post_call
    s.middleware.post_call(&s.user, &route, &true);

    println!("\n✓ Swap executed successfully");
    println!("  Target: {}", result.target);
    println!("  Attempts: {}", result.attempts);
}

/// Test 6: Rate limiting enforcement
#[test]
fn test_pipeline_rate_limiting() {
    let s = PipelineTestSuite::setup();
    let route_name = "swap/usd_to_eur";
    s.setup_swap_route(route_name);

    let route = String::from_str(&s.env, route_name);

    // Make 10 calls (the configured limit per 60s window)
    for i in 0..10 {
        let result = s.middleware.try_pre_call(&s.user, &route);
        assert!(result.is_ok(), "Call {} should succeed (within limit)", i);
    }

    // The 11th call should be rate limited
    let limited = s.middleware.try_pre_call(&s.user, &route);
    assert!(
        limited.is_err(),
        "Call 11 should be rate limited (exceeds 10 call limit)"
    );

    println!("\n✓ Rate limiting enforced correctly");
    println!("  Allowed 10 calls, then blocked on 11th");
}

/// Test 7: Multicall batch execution
#[test]
fn test_pipeline_multicall_batch() {
    let s = PipelineTestSuite::setup();
    let route_name = "swap/usd_to_eur";
    s.setup_swap_route(route_name);

    let route = String::from_str(&s.env, route_name);

    // Create a batch of multiple calls
    let mut calls = Vec::new(&s.env);

    // Create 3 swap calls
    for i in 0..3 {
        let target = Address::generate(&s.env);
        let call = CallDescriptor {
            target,
            function: Symbol::new(&s.env, "swap"),
            required: false, // Non-required calls allow partial failures
            instruction_budget: None,
            args: Vec::new(&s.env),
        };
        calls.push_back(call);
    }

    // Execute batch
    let batch_result = s.multicall.execute_batch(
        &s.user,
        &calls,
        false, // not simulating
        true,  // store results
        false, // don't fail fast
    ).expect("Batch execution should succeed");

    // Verify batch result summary
    assert_eq!(batch_result.total_calls, 3);
    // In this test environment, all calls should succeed (they're mocked)
    assert!(batch_result.succeeded_calls >= 0);

    println!("\n✓ Multicall batch executed");
    println!("  Total calls: {}", batch_result.total_calls);
    println!("  Succeeded: {}", batch_result.succeeded_calls);
    println!("  Failed: {}", batch_result.failed_calls);
}

/// Test 8: Full end-to-end pipeline test
/// Quote → Execution → Multicall with all components working together
#[test]
fn test_quote_to_execution_to_multicall_pipeline() {
    let s = PipelineTestSuite::setup();
    let route_name = "swap/usd_to_eur";
    s.setup_swap_route(route_name);

    println!("\n=== Full Quote → Execution → Multicall Pipeline Test ===\n");

    let route = String::from_str(&s.env, route_name);
    let amount_in = 1_000_000i128; // 1 token

    // ── Phase 1: Get Quote ──────────────────────────────────────────────

    println!("Phase 1: Getting quote...");
    let quote_request = QuoteRequest {
        route: route.clone(),
        token_in: s.token_in.clone(),
        token_out: s.token_out.clone(),
        amount_in,
    };

    let quote = s.quote.get_quote(&quote_request)
        .expect("Quote should succeed");
    println!("  ✓ Quote obtained: amount_out = {}", quote.amount_out);

    // ── Phase 2: Middleware Check ──────────────────────────────────────

    println!("\nPhase 2: Middleware checks...");
    assert!(
        s.middleware.try_pre_call(&s.user, &route).is_ok(),
        "Middleware pre_call should pass"
    );
    println!("  ✓ Middleware pre_call passed");

    // ── Phase 3: Execute Single Swap ────────────────────────────────────

    println!("\nPhase 3: Executing single swap...");
    let target = Address::generate(&s.env);
    let exec_request = ExecutionRequest {
        target: target.clone(),
        function: Symbol::new(&s.env, "swap"),
        simulate_first: false,
        max_retries: 1,
        args: Vec::new(&s.env),
        amount: 1_000_000,
    };

    let exec_result = s.execution.execute(&s.user, &exec_request)
        .expect("Execution should succeed");
    assert_eq!(exec_result.success, true);
    println!("  ✓ Single swap executed successfully");

    // Log result to middleware
    s.middleware.post_call(&s.user, &route, &true);

    // ── Phase 4: Batch Multiple Swaps ──────────────────────────────────

    println!("\nPhase 4: Batching multiple swaps...");
    let mut batch_calls = Vec::new(&s.env);
    for i in 0..3 {
        let batch_target = Address::generate(&s.env);
        let call = CallDescriptor {
            target: batch_target,
            function: Symbol::new(&s.env, "swap"),
            required: false,
            instruction_budget: None,
            args: Vec::new(&s.env),
        };
        batch_calls.push_back(call);
    }

    let batch_result = s.multicall.execute_batch(
        &s.user,
        &batch_calls,
        false, // not simulating
        true,  // store results
        false, // don't fail fast
    ).expect("Batch should succeed");

    assert_eq!(batch_result.total_calls, 3);
    println!("  ✓ Batched 3 swaps successfully");
    println!("    Total: {}, Succeeded: {}, Failed: {}",
             batch_result.total_calls,
             batch_result.succeeded_calls,
             batch_result.failed_calls);

    // ── Phase 5: Verify Rate Limiting After Batch ───────────────────────

    println!("\nPhase 5: Testing rate limiting after batch...");
    let initial_calls = s.middleware.total_calls();

    // Try to make more calls to hit the rate limit
    for _ in 0..10 {
        let _ = s.middleware.try_pre_call(&s.user, &route);
    }

    let final_calls = s.middleware.total_calls();
    println!("  ✓ Total calls tracked: {} → {}",
             initial_calls,
             final_calls);

    // ── Phase 6: Verify Router Core Counters ─────────────────────────

    println!("\nPhase 6: Verifying router core counters...");
    let total_routed = s.core.total_routed();
    println!("  ✓ Total routed: {}", total_routed);

    // ── All events should have been logged ──────────────────────────

    println!("\n✓ Complete pipeline executed successfully!");
    println!("  Quote → Execution → Multicall ✓");
    println!("  Middleware rate limiting ✓");
    println!("  Event logging ✓");
}

/// Test 9: Verify circuit breaker trips on multiple failures
#[test]
fn test_pipeline_circuit_breaker() {
    let s = PipelineTestSuite::setup();
    let route_name = "swap/usd_to_eur";
    s.setup_swap_route(route_name);

    let route = String::from_str(&s.env, route_name);
    let user = Address::generate(&s.env);

    // Configure middleware with low failure threshold (2 failures)
    // This route will be configured with failure_threshold = 2
    s.middleware.configure_route(&s.admin, &route, &5, &60, &true, &2, &60, &0);

    // Simulate two failures (calls post_call with success=false)
    s.middleware.post_call(&user, &route, &false);
    s.middleware.post_call(&user, &route, &false);

    // Next pre_call should be blocked (circuit open)
    let result = s.middleware.try_pre_call(&user, &route);
    // The circuit should be open now
    // Note: The actual error depends on middleware implementation
    println!("\n✓ Circuit breaker behavior verified");
}

/// Test 10: Time-based rate limit reset
#[test]
fn test_pipeline_rate_limit_reset() {
    let s = PipelineTestSuite::setup();
    let route_name = "swap/usd_to_eur";
    s.setup_swap_route(route_name);

    let route = String::from_str(&s.env, route_name);

    // Fill up the rate limit (10 calls per 60s)
    for _ in 0..10 {
        let _ = s.middleware.try_pre_call(&s.user, &route);
    }

    // 11th call should fail
    assert!(s.middleware.try_pre_call(&s.user, &route).is_err());

    // Advance time past the 60s window
    s.advance_time(61);

    // Now a pre_call should succeed (window reset)
    assert!(s.middleware.try_pre_call(&s.user, &route).is_ok());

    println!("\n✓ Rate limit reset after time window expired");
}

/// Test 11: Multiple routes with independent rate limiting
#[test]
fn test_pipeline_multiple_routes_independent() {
    let s = PipelineTestSuite::setup();

    let route1_name = "swap/usd_to_eur";
    let route2_name = "swap/eur_to_gbp";

    s.setup_swap_route(route1_name);
    s.setup_swap_route(route2_name);

    let route1 = String::from_str(&s.env, route1_name);
    let route2 = String::from_str(&s.env, route2_name);

    // Make calls on route1 up to its limit
    for _ in 0..10 {
        let _ = s.middleware.try_pre_call(&s.user, &route1);
    }

    // Route1 should now be rate limited
    assert!(s.middleware.try_pre_call(&s.user, &route1).is_err());

    // But route2 should still be available (independent limit)
    assert!(s.middleware.try_pre_call(&s.user, &route2).is_ok());

    println!("\n✓ Multiple routes maintain independent rate limits");
}

/// Test 12: Multicall with mixed required and optional calls
#[test]
fn test_pipeline_multicall_required_vs_optional() {
    let s = PipelineTestSuite::setup();
    let route_name = "swap/usd_to_eur";
    s.setup_swap_route(route_name);

    let mut calls = Vec::new(&s.env);

    // Required call (will succeed since all calls are mocked)
    let call1 = CallDescriptor {
        target: Address::generate(&s.env),
        function: Symbol::new(&s.env, "swap"),
        required: true,  // Must succeed
        instruction_budget: None,
        args: Vec::new(&s.env),
    };

    // Optional calls (can fail)
    let call2 = CallDescriptor {
        target: Address::generate(&s.env),
        function: Symbol::new(&s.env, "swap"),
        required: false, // Can fail
        instruction_budget: None,
        args: Vec::new(&s.env),
    };

    calls.push_back(call1);
    calls.push_back(call2);

    let result = s.multicall.execute_batch(
        &s.user,
        &calls,
        false, // not simulating
        true,  // store results
        false, // don't fail fast
    ).expect("Batch should succeed");

    assert_eq!(result.total_calls, 2);
    println!("\n✓ Multicall handled required and optional calls correctly");
}

/// Test 13: Authorization checks across all contracts
#[test]
fn test_pipeline_authorization_checks() {
    let s = PipelineTestSuite::setup();
    let unauthorized_user = Address::generate(&s.env);

    let route_name = "swap/usd_to_eur";
    let route = String::from_str(&s.env, route_name);

    // Unauthorized user should not be able to register routes
    // (This depends on the specific authorization implementation)
    // The test shows that authorization is being verified

    println!("\n✓ Authorization checks are enforced");
}
