//! Integration tests for route expiry behavior across contracts.
//!
//! Covers the scenarios from issue #732:
//! - Resolve an expired route (RouteExpired)
//! - batch_resolve with a mix of active and expired routes
//! - extend_route_ttl fails on an already-expired route
//! - Permanent routes are never treated as expired
//!
//! Note: these tests run against Stellar testnet and require the WASM
//! contracts to be built. The ledger sequence on testnet advances with
//! each block, so TTL values of 0 ledgers cause a route to expire
//! immediately (current ledger already exceeds expires_at after one block).
//!
//! Run with:
//!   cargo test --test integration -- route_expiry --ignored --test-threads=1

use integration_tests::{TestAccount, TestSuite};

/// Helper: register a route with a TTL of `ttl_ledgers` (0 = expires immediately).
fn register_with_ttl(
    core: &integration_tests::DeployedContract,
    admin: &TestAccount,
    name: &str,
    addr: &str,
    ttl: u32,
) -> Result<String, String> {
    core.invoke(
        "register_route_with_ttl",
        &[
            "--caller", &admin.address,
            "--name", name,
            "--address", addr,
            "--ttl_ledgers", &ttl.to_string(),
        ],
        admin,
    )
}

/// Helper: register a permanent route (no TTL).
fn register_permanent(
    core: &integration_tests::DeployedContract,
    admin: &TestAccount,
    name: &str,
    addr: &str,
) -> Result<String, String> {
    core.invoke(
        "register_route",
        &[
            "--caller", &admin.address,
            "--name", name,
            "--address", addr,
            "--metadata", "null",
        ],
        admin,
    )
}

// ── Test 1: resolve an expired route returns RouteExpired ─────────────────────

#[test]
#[ignore]
fn test_resolve_expired_route_returns_error() {
    println!("\n=== Test: Resolve expired route ===\n");

    let fixture = TestSuite::setup().expect("setup failed");
    let core = fixture.router_core.as_ref().expect("core not deployed");
    let admin = &fixture.admin;

    let target = TestAccount::generate().expect("gen target").address;

    // Register with TTL = 0 ledgers → expires at current ledger; the very
    // next block advances sequence past it.
    register_with_ttl(core, admin, "expiring-route", &target, 0)
        .expect("register_route_with_ttl");

    // Wait for one ledger to advance so the route is expired.
    std::thread::sleep(std::time::Duration::from_secs(6));

    let result = core.try_invoke("resolve", &["--name", "expiring-route"], admin);

    // On testnet try_invoke returns Err when the contract panics.
    // The error output contains the contract error code or name.
    let err = result.expect_err("resolve on expired route must fail");
    assert!(
        err.contains("RouteExpired") || err.contains("13"),
        "expected RouteExpired error, got: {}",
        err
    );
    println!("✓ Expired route correctly rejected: {}", err);
}

// ── Test 2: permanent route never expires ────────────────────────────────────

#[test]
#[ignore]
fn test_permanent_route_never_expires() {
    println!("\n=== Test: Permanent route does not expire ===\n");

    let fixture = TestSuite::setup().expect("setup failed");
    let core = fixture.router_core.as_ref().expect("core not deployed");
    let admin = &fixture.admin;

    let target = TestAccount::generate().expect("gen target").address;
    register_permanent(core, admin, "permanent-route", &target).expect("register permanent");

    // Let some ledgers pass.
    std::thread::sleep(std::time::Duration::from_secs(8));

    let resolved = core
        .invoke("resolve", &["--name", "permanent-route"], admin)
        .expect("permanent route must still resolve");

    assert!(
        resolved.contains(&target),
        "resolved address mismatch: {}",
        resolved
    );
    println!("✓ Permanent route still resolves after ledger advance: {}", resolved);
}

// ── Test 3: batch_resolve with mixed active and expired routes ────────────────

#[test]
#[ignore]
fn test_batch_resolve_mixed_expiry() {
    println!("\n=== Test: batch_resolve with active and expired routes ===\n");

    let fixture = TestSuite::setup().expect("setup failed");
    let core = fixture.router_core.as_ref().expect("core not deployed");
    let admin = &fixture.admin;

    let active_addr = TestAccount::generate().expect("gen active").address;
    let expired_addr = TestAccount::generate().expect("gen expired").address;

    register_permanent(core, admin, "batch-active", &active_addr).expect("register active");
    register_with_ttl(core, admin, "batch-expired", &expired_addr, 0)
        .expect("register expiring");

    // Let the short-TTL route expire.
    std::thread::sleep(std::time::Duration::from_secs(6));

    // batch_resolve returns a JSON array; expired routes map to RouteNotFound.
    let result = core
        .invoke(
            "batch_resolve",
            &["--names", r#"["batch-active","batch-expired"]"#],
            admin,
        )
        .expect("batch_resolve must not panic");

    println!("batch_resolve result: {}", result);

    // The active route should appear as Ok(address) in the output.
    assert!(
        result.contains(&active_addr) || result.contains("Ok"),
        "active route missing from batch result: {}",
        result
    );
    // The expired route should map to an error variant.
    assert!(
        result.contains("RouteNotFound") || result.contains("Err"),
        "expired route should be an error in batch result: {}",
        result
    );
    println!("✓ batch_resolve correctly distinguishes active and expired routes");
}

// ── Test 4: extend_route_ttl fails if route already expired ──────────────────

#[test]
#[ignore]
fn test_extend_ttl_fails_after_expiry() {
    println!("\n=== Test: extend_route_ttl on already-expired route ===\n");

    let fixture = TestSuite::setup().expect("setup failed");
    let core = fixture.router_core.as_ref().expect("core not deployed");
    let admin = &fixture.admin;

    let target = TestAccount::generate().expect("gen target").address;
    register_with_ttl(core, admin, "extend-expired", &target, 0).expect("register with ttl 0");

    std::thread::sleep(std::time::Duration::from_secs(6));

    let result = core.try_invoke(
        "extend_route_ttl",
        &[
            "--caller", &admin.address,
            "--name", "extend-expired",
            "--additional_ledgers", "100",
        ],
        admin,
    );

    let err = result.expect_err("extend_route_ttl on expired route must fail");
    assert!(
        err.contains("RouteExpired") || err.contains("13"),
        "expected RouteExpired, got: {}",
        err
    );
    println!("✓ extend_route_ttl correctly rejects expired route: {}", err);
}

// ── Test 5: extend_route_ttl succeeds before expiry ──────────────────────────

#[test]
#[ignore]
fn test_extend_ttl_before_expiry_succeeds() {
    println!("\n=== Test: extend_route_ttl before expiry ===\n");

    let fixture = TestSuite::setup().expect("setup failed");
    let core = fixture.router_core.as_ref().expect("core not deployed");
    let admin = &fixture.admin;

    let target = TestAccount::generate().expect("gen target").address;
    // 1000 ledgers ≈ ~83 minutes — well within test duration.
    register_with_ttl(core, admin, "extend-active", &target, 1000).expect("register with ttl");

    core.invoke(
        "extend_route_ttl",
        &[
            "--caller", &admin.address,
            "--name", "extend-active",
            "--additional_ledgers", "500",
        ],
        admin,
    )
    .expect("extend_route_ttl before expiry must succeed");

    // Route should still be resolvable.
    let resolved = core
        .invoke("resolve", &["--name", "extend-active"], admin)
        .expect("route must resolve after TTL extension");

    assert!(resolved.contains(&target), "resolved address mismatch: {}", resolved);
    println!("✓ extend_route_ttl succeeded; route still resolves: {}", resolved);
}
