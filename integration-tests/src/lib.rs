//! Shared integration test helpers for stellar-router.

use std::env;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

/// Absolute path to a release WASM artifact, resolved relative to the
/// workspace root rather than the test process's current working directory.
///
/// `integration-tests` is its own standalone Cargo workspace, so
/// `cargo test --manifest-path integration-tests/Cargo.toml` runs the test
/// binary with its CWD set to `integration-tests/` (cargo always uses the
/// invoked manifest's directory, not the caller's own cwd) — while the
/// contracts are built at the repo root's `target/`. A bare
/// `"target/wasm32-unknown-unknown/release/..."` literal therefore silently
/// resolves to `integration-tests/target/...`, which never exists.
pub fn wasm_path(contract_name: &str) -> String {
    format!(
        "{}/../target/wasm32-unknown-unknown/release/{contract_name}.wasm",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Configuration for Stellar testnet integration tests.
///
/// Only the network name is configurable (via `STELLAR_NETWORK`). It is
/// passed to stellar-cli as `--network`, which resolves the matching RPC URL
/// and network passphrase from the CLI's own network config.
#[derive(Debug, Clone)]
pub struct TestnetConfig {
    pub network: String,
}

impl Default for TestnetConfig {
    fn default() -> Self {
        Self {
            network: env::var("STELLAR_NETWORK").unwrap_or_else(|_| "testnet".to_string()),
        }
    }
}

/// Monotonic counter so accounts generated within the same test process
/// (and even within the same second) never collide on identity name.
static ACCOUNT_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Test account: a named identity in the local stellar-cli keystore.
///
/// `stellar keys generate` writes the keypair to `stellar-cli`'s own config
/// directory rather than printing it to stdout, and `--source` on
/// deploy/invoke must be an identity name (or a raw secret key) — a bare
/// public key cannot sign transactions. So `name` (not `address`) is what
/// gets passed to `--source`; `address` is only for display/contract-call
/// arguments that expect a public key.
///
/// The identity is removed from the keystore (`stellar keys rm`) when the
/// last clone of the account is dropped, so test runs don't leave key files
/// behind. Moving `address` out of a temporary account (e.g.
/// `TestAccount::generate()?.address`) removes the identity immediately,
/// which is fine when only the public key is needed.
#[derive(Debug, Clone)]
pub struct TestAccount {
    pub name: String,
    pub address: String,
    _identity: Arc<IdentityGuard>,
}

/// Removes a generated stellar-cli identity when dropped.
#[derive(Debug)]
struct IdentityGuard {
    name: String,
}

impl IdentityGuard {
    fn remove(name: &str) {
        // Best-effort cleanup: ignore failures (e.g. CLI missing or key already gone).
        let _ = Command::new("stellar").args(["keys", "rm", name]).output();
    }
}

impl Drop for IdentityGuard {
    fn drop(&mut self) {
        Self::remove(&self.name);
    }
}

impl TestAccount {
    /// Generate a new test account (a uniquely-named local identity).
    pub fn generate() -> Result<Self, String> {
        let name = format!(
            "it-{}-{}",
            std::process::id(),
            ACCOUNT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );

        let output = Command::new("stellar")
            .args(["keys", "generate", &name])
            .output()
            .map_err(|e| format!("Failed to generate keypair: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "stellar keys generate failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // From here on the identity exists in the keystore; the guard removes
        // it on early return as well as when the account is dropped.
        let guard = Arc::new(IdentityGuard { name: name.clone() });

        let address_output = Command::new("stellar")
            .args(["keys", "address", &name])
            .output()
            .map_err(|e| format!("Failed to look up generated address: {}", e))?;

        if !address_output.status.success() {
            return Err(format!(
                "stellar keys address failed: {}",
                String::from_utf8_lossy(&address_output.stderr)
            ));
        }

        let address = String::from_utf8_lossy(&address_output.stdout)
            .trim()
            .to_string();

        Ok(Self {
            name,
            address,
            _identity: guard,
        })
    }

    /// Fund this account using Friendbot.
    pub fn fund(&self, network: &str) -> Result<(), String> {
        let output = Command::new("stellar")
            .args(["keys", "fund", &self.name, "--network", network])
            .output()
            .map_err(|e| format!("Failed to fund account: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Friendbot funding failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        std::thread::sleep(Duration::from_secs(2));
        Ok(())
    }
}

/// Deployed contract instance.
#[derive(Debug, Clone)]
pub struct DeployedContract {
    pub contract_id: String,
    pub wasm_path: String,
    pub name: String,
    pub network: String,
}

impl DeployedContract {
    /// Deploy a contract to testnet.
    pub fn deploy(
        wasm_path: &str,
        name: &str,
        source_account: &TestAccount,
        network: &str,
    ) -> Result<Self, String> {
        let output = Command::new("stellar")
            .args([
                "contract",
                "deploy",
                "--wasm",
                wasm_path,
                "--network",
                network,
                "--source",
                &source_account.name,
            ])
            .output()
            .map_err(|e| format!("Failed to deploy contract: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Contract deployment failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let contract_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        std::thread::sleep(Duration::from_secs(2));

        Ok(Self {
            contract_id,
            wasm_path: wasm_path.to_string(),
            name: name.to_string(),
            network: network.to_string(),
        })
    }

    /// Build and run a `stellar contract invoke` command, returning the raw process output.
    fn run_invoke(
        &self,
        method: &str,
        args: &[&str],
        source_account: &TestAccount,
    ) -> Result<std::process::Output, String> {
        let mut cmd_args = vec![
            "contract",
            "invoke",
            "--id",
            &self.contract_id,
            "--network",
            &self.network,
            "--source",
            &source_account.name,
            "--",
            method,
        ];
        cmd_args.extend_from_slice(args);

        Command::new("stellar")
            .args(&cmd_args)
            .output()
            .map_err(|e| format!("Failed to invoke contract: {}", e))
    }

    /// Invoke a contract method.
    pub fn invoke(
        &self,
        method: &str,
        args: &[&str],
        source_account: &TestAccount,
    ) -> Result<String, String> {
        let output = self.run_invoke(method, args, source_account)?;

        if !output.status.success() {
            return Err(format!(
                "Contract invocation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Try to invoke a contract method, expecting it to fail.
    pub fn try_invoke(
        &self,
        method: &str,
        args: &[&str],
        source_account: &TestAccount,
    ) -> Result<String, String> {
        let output = self.run_invoke(method, args, source_account)?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        if !output.status.success() {
            Err(stderr)
        } else {
            Ok(stdout)
        }
    }
}

/// Default fee (basis points) the suite initializes router-quote with.
pub const DEFAULT_QUOTE_FEE_BPS: u32 = 30;

/// Shared integration-test fixture that deploys and initializes all contracts.
pub struct TestSuite {
    pub config: TestnetConfig,
    pub admin: TestAccount,
    pub user1: TestAccount,
    pub user2: TestAccount,
    pub router_core: Option<DeployedContract>,
    pub router_registry: Option<DeployedContract>,
    pub router_access: Option<DeployedContract>,
    pub router_middleware: Option<DeployedContract>,
    pub router_timelock: Option<DeployedContract>,
    pub router_multicall: Option<DeployedContract>,
    pub router_quote: Option<DeployedContract>,
    pub router_execution: Option<DeployedContract>,
}

impl TestSuite {
    /// Build and fully initialize the suite.
    pub fn setup() -> Result<Self, String> {
        let mut suite = Self::new()?;
        suite.deploy_all_contracts()?;
        suite.initialize_all_contracts()?;
        Ok(suite)
    }

    /// Cleanup hook for local runs.
    ///
    /// The suite's generated identities (`admin`, `user1`, `user2`) are
    /// removed from the stellar-cli keystore when their `TestAccount`s are
    /// dropped, which happens right after this runs from `Drop`.
    pub fn teardown(&self) {
        println!("Test suite teardown complete");
    }

    pub fn core(&self) -> Result<&DeployedContract, String> {
        self.router_core
            .as_ref()
            .ok_or("Core contract not deployed".to_string())
    }

    pub fn registry(&self) -> Result<&DeployedContract, String> {
        self.router_registry
            .as_ref()
            .ok_or("Registry contract not deployed".to_string())
    }

    pub fn access(&self) -> Result<&DeployedContract, String> {
        self.router_access
            .as_ref()
            .ok_or("Access contract not deployed".to_string())
    }

    pub fn middleware(&self) -> Result<&DeployedContract, String> {
        self.router_middleware
            .as_ref()
            .ok_or("Middleware contract not deployed".to_string())
    }

    pub fn timelock(&self) -> Result<&DeployedContract, String> {
        self.router_timelock
            .as_ref()
            .ok_or("Timelock contract not deployed".to_string())
    }

    pub fn multicall(&self) -> Result<&DeployedContract, String> {
        self.router_multicall
            .as_ref()
            .ok_or("Multicall contract not deployed".to_string())
    }

    pub fn quote(&self) -> Result<&DeployedContract, String> {
        self.router_quote
            .as_ref()
            .ok_or("Quote contract not deployed".to_string())
    }

    pub fn execution(&self) -> Result<&DeployedContract, String> {
        self.router_execution
            .as_ref()
            .ok_or("Execution contract not deployed".to_string())
    }

    fn new() -> Result<Self, String> {
        let config = TestnetConfig::default();

        let admin = TestAccount::generate()?;
        admin.fund(&config.network)?;

        let user1 = TestAccount::generate()?;
        user1.fund(&config.network)?;

        let user2 = TestAccount::generate()?;
        user2.fund(&config.network)?;

        Ok(Self {
            config,
            admin,
            user1,
            user2,
            router_core: None,
            router_registry: None,
            router_access: None,
            router_middleware: None,
            router_timelock: None,
            router_multicall: None,
            router_quote: None,
            router_execution: None,
        })
    }

    fn deploy_all_contracts(&mut self) -> Result<(), String> {
        let network = &self.config.network;

        self.router_registry = Some(DeployedContract::deploy(
            &wasm_path("router_registry"),
            "router-registry",
            &self.admin,
            network,
        )?);

        self.router_access = Some(DeployedContract::deploy(
            &wasm_path("router_access"),
            "router-access",
            &self.admin,
            network,
        )?);

        self.router_middleware = Some(DeployedContract::deploy(
            &wasm_path("router_middleware"),
            "router-middleware",
            &self.admin,
            network,
        )?);

        self.router_timelock = Some(DeployedContract::deploy(
            &wasm_path("router_timelock"),
            "router-timelock",
            &self.admin,
            network,
        )?);

        self.router_multicall = Some(DeployedContract::deploy(
            &wasm_path("router_multicall"),
            "router-multicall",
            &self.admin,
            network,
        )?);

        self.router_core = Some(DeployedContract::deploy(
            &wasm_path("router_core"),
            "router-core",
            &self.admin,
            network,
        )?);

        self.router_quote = Some(DeployedContract::deploy(
            &wasm_path("router_quote"),
            "router-quote",
            &self.admin,
            network,
        )?);

        self.router_execution = Some(DeployedContract::deploy(
            &wasm_path("router_execution"),
            "router-execution",
            &self.admin,
            network,
        )?);

        Ok(())
    }

    fn initialize_all_contracts(&self) -> Result<(), String> {
        if let Some(ref core) = self.router_core {
            core.invoke("initialize", &["--admin", &self.admin.address], &self.admin)?;
        }

        if let Some(ref registry) = self.router_registry {
            registry.invoke("initialize", &["--admin", &self.admin.address], &self.admin)?;
        }

        if let Some(ref access) = self.router_access {
            access.invoke(
                "initialize",
                &["--super_admin", &self.admin.address],
                &self.admin,
            )?;
        }

        if let Some(ref middleware) = self.router_middleware {
            middleware.invoke("initialize", &["--admin", &self.admin.address], &self.admin)?;
        }

        if let Some(ref timelock) = self.router_timelock {
            timelock.invoke(
                "initialize",
                &[
                    "--admin",
                    &self.admin.address,
                    "--min_delay",
                    "60",
                    "--max_pending_ops",
                    "0",
                ],
                &self.admin,
            )?;
        }

        if let Some(ref multicall) = self.router_multicall {
            multicall.invoke(
                "initialize",
                &["--admin", &self.admin.address, "--max_batch_size", "10"],
                &self.admin,
            )?;
        }

        if let Some(ref quote) = self.router_quote {
            quote.invoke(
                "initialize",
                &[
                    "--admin",
                    &self.admin.address,
                    "--default_fee_bps",
                    &DEFAULT_QUOTE_FEE_BPS.to_string(),
                ],
                &self.admin,
            )?;
        }

        if let Some(ref execution) = self.router_execution {
            execution.invoke(
                "initialize",
                &[
                    "--admin",
                    &self.admin.address,
                    "--max_retries",
                    "3",
                    "--backoff_base_ms",
                    "0",
                    "--backoff_multiplier",
                    "200",
                ],
                &self.admin,
            )?;
        }

        Ok(())
    }
}

impl Drop for TestSuite {
    fn drop(&mut self) {
        self.teardown();
    }
}
