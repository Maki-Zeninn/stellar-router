//! Integration tests for stellar-router
//!
//! These tests run against Stellar testnet and verify end-to-end functionality.
//!
//! ## Running the tests
//!
//! ### Prerequisites
//! 1. Install stellar-cli: `cargo install --locked stellar-cli`
//! 2. Build WASM contracts: `cargo build --target wasm32-unknown-unknown --release`
//!
//! ### Run all integration tests
//! ```bash
//! cargo test --test integration_tests -- --ignored --test-threads=1
//! ```
//!
//! ### Run specific test
//! ```bash
//! cargo test --test integration_tests test_full_router_core_flow -- --ignored
//! ```
//!
//! ### Run with output
//! ```bash
//! cargo test --test integration_tests -- --ignored --nocapture --test-threads=1
//! ```
//!
//! ## Test Organization
//!
//! - `testnet_setup.rs` - Utilities for deploying and managing testnet resources
//! - `full_flow_test.rs` - Happy path end-to-end tests
//! - `failure_scenarios.rs` - Error handling and edge case tests
//!
//! ## Notes
//!
//! - Tests are marked with `#[ignore]` to prevent running in normal CI
//! - Use `--test-threads=1` to avoid testnet rate limits
//! - Each test creates fresh accounts via Friendbot
//! - Tests clean up after themselves but may leave contracts on testnet

mod integration;

#[cfg(test)]
mod quick_tests {
    use integration_tests::{wasm_path, DeployedContract, TestAccount};

    #[test]
    #[ignore]
    fn test_stellar_cli_available() {
        let output = std::process::Command::new("stellar")
            .arg("--version")
            .output()
            .expect("Failed to run stellar CLI - is it installed?");

        assert!(output.status.success(), "stellar CLI not working properly");
        let version = String::from_utf8_lossy(&output.stdout);
        println!("stellar CLI version: {}", version);
        assert!(version.contains("stellar"), "Unexpected stellar CLI output");
    }

    #[test]
    #[ignore]
    fn test_wasm_contracts_built() {
        let contracts = vec![
            wasm_path("router_core"),
            wasm_path("router_registry"),
            wasm_path("router_access"),
            wasm_path("router_middleware"),
            wasm_path("router_timelock"),
            wasm_path("router_multicall"),
            wasm_path("router_quote"),
            wasm_path("router_execution"),
        ];

        for contract in contracts {
            assert!(
                std::path::Path::new(&contract).exists(),
                "Contract not found: {}. Run: cargo build --target wasm32-unknown-unknown --release",
                contract
            );
        }
        println!("✓ All WASM contracts found");
    }

    #[test]
    #[ignore]
    fn test_account_generation_and_funding() {
        println!("\n=== Testing Account Setup ===\n");

        let account = TestAccount::generate().expect("Failed to generate test account");
        println!("✓ Generated account: {}", account.address);

        account
            .fund("testnet")
            .expect("Failed to fund account via Friendbot");
        println!("✓ Account funded successfully");

        println!("\n=== Account Setup Test PASSED ===\n");
    }

    /// End-to-end: deploy router-core, initialize, register a route, resolve it,
    /// and assert the returned address matches what was registered.
    #[test]
    #[ignore]
    fn test_router_core_register_and_resolve() {
        let network = "testnet";
        let wasm = wasm_path("router_core");
        assert!(
            std::path::Path::new(&wasm).exists(),
            "router_core.wasm not found — run: cargo build --target wasm32-unknown-unknown --release"
        );

        let admin = TestAccount::generate().expect("generate admin");
        admin.fund(network).expect("fund admin");

        let core = DeployedContract::deploy(&wasm, "router-core", &admin, network)
            .expect("deploy router-core");

        core.invoke("initialize", &["--admin", &admin.address], &admin)
            .expect("initialize router-core");

        // Use a freshly generated address as the mock contract target
        let target = TestAccount::generate().expect("generate target").address;

        core.invoke(
            "register_route",
            &[
                "--caller",
                &admin.address,
                "--name",
                "oracle",
                "--address",
                &target,
                "--metadata",
                "null",
            ],
            &admin,
        )
        .expect("register_route");

        let resolved = core
            .invoke("resolve", &["--name", "oracle"], &admin)
            .expect("resolve");

        assert!(
            resolved.contains(&target),
            "resolved address '{}' does not match registered target '{}'",
            resolved,
            target
        );
        println!("✓ router-core register+resolve PASSED: {}", resolved);
    }

    /// End-to-end: deploy router-middleware, configure max_calls_per_window=2,
    /// call pre_call three times (third must return RateLimitExceeded),
    /// then advance the ledger past the window and assert the call succeeds.
    #[test]
    #[ignore]
    fn test_middleware_rate_limit_exceeded_then_resets() {
        let network = "testnet";
        let wasm = wasm_path("router_middleware");
        assert!(
            std::path::Path::new(&wasm).exists(),
            "router_middleware.wasm not found — run: cargo build --target wasm32-unknown-unknown --release"
        );

        let admin = TestAccount::generate().expect("generate admin");
        admin.fund(network).expect("fund admin");

        let mw = DeployedContract::deploy(&wasm, "router-middleware", &admin, network)
            .expect("deploy router-middleware");

        mw.invoke("initialize", &["--admin", &admin.address], &admin)
            .expect("initialize");

        // Configure route: max 2 calls per 60-second window
        mw.invoke(
            "configure_route",
            &[
                "--caller",
                &admin.address,
                "--route",
                "oracle/get_price",
                "--max_calls_per_window",
                "2",
                "--window_seconds",
                "60",
                "--enabled",
                "true",
                "--failure_threshold",
                "0",
                "--recovery_window_seconds",
                "0",
                "--log_retention",
                "0",
            ],
            &admin,
        )
        .expect("configure_route");

        let caller = TestAccount::generate().expect("generate caller");
        caller.fund(network).expect("fund caller");

        // Call 1 — should succeed
        mw.invoke(
            "pre_call",
            &["--caller", &caller.address, "--route", "oracle/get_price"],
            &caller,
        )
        .expect("pre_call 1");

        // Call 2 — should succeed
        mw.invoke(
            "pre_call",
            &["--caller", &caller.address, "--route", "oracle/get_price"],
            &caller,
        )
        .expect("pre_call 2");

        // Call 3 — must fail with RateLimitExceeded
        let err = mw
            .try_invoke(
                "pre_call",
                &["--caller", &caller.address, "--route", "oracle/get_price"],
                &caller,
            )
            .expect_err("pre_call 3 should fail with RateLimitExceeded");
        
        // stellar-cli prints contract errors like "Error(Contract, #4)" where 4 is the error code
        // RateLimitExceeded is error variant #4 in MiddlewareError enum
        assert!(
            err.contains("Error(Contract, #4)") || err.contains("RateLimitExceeded"),
            "expected RateLimitExceeded error, got: {}",
            err
        );
        println!("✓ Third call correctly rejected: {}", err);

        // Note: advancing ledger time on testnet is not possible via CLI.
        // The window-reset behaviour is verified by the unit test
        // `test_rate_limit_resets_after_window` in router-middleware.
        println!("✓ middleware rate-limit end-to-end PASSED");
    }
}
