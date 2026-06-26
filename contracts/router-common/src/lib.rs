#![no_std]

//! # router-common
//!
//! Shared macros and utilities for the stellar-router suite.
//!
//! ## Macros
//! - [`require_admin!`] — inline admin check used across router contracts
//! - [`require_admin_simple!`] — convenience version with standard error variants
//! - [`require_admin_simple!`] — convenience macro for standard DataKey::Admin and error variants
//! - [`admin_transfer_complete!`] — shared admin transfer pattern (storage set + event emit)
//!
//! ## Storage Helpers
//!
//! Shared instance-storage patterns are consolidated to remove per-contract
//! boilerplate while keeping each contract's own `DataKey` enum:
//! - [`CommonDataKey`] — shared `Admin` storage key
//! - [`get_admin`] / [`set_admin`] — admin address accessors
//! - [`is_initialized`] / [`require_initialized`] / [`require_uninitialized`] — initialization guards
//! - [`extend_instance_ttl`] — instance-storage TTL extension wrapper
//! - [`StorageHelper`] — trait with default implementations of the above
//!
//! ## Event Topic Naming Convention
//!
//! All event topics across stellar-router contracts follow these rules:
//!
//! 1. **Use snake_case** - All event topics must use lowercase with underscores
//!    - ✅ Good: `route_registered`, `admin_transferred`, `role_granted`
//!    - ❌ Bad: `routeRegistered`, `AdminTransferred`, `RoleGranted`
//!
//! 2. **Use descriptive past-tense verbs** - Events represent actions that have occurred
//!    - ✅ Good: `route_registered`, `role_revoked`, `circuit_opened`
//!    - ❌ Bad: `register_route`, `revoke_role`, `open_circuit`
//!
//! 3. **Be specific and unambiguous** - Event names should clearly indicate what happened
//!    - ✅ Good: `max_batch_size_updated`, `route_resolve_paused`
//!    - ❌ Bad: `updated`, `paused`
//!
//! 4. **Use full words, avoid abbreviations** - Clarity over brevity
//!    - ✅ Good: `execution_result`, `simulation_result`
//!    - ❌ Bad: `exec_result`, `sim_result`
//!
//! 5. **Consistent terminology** - Use the same terms across related events
//!    - Admin events: `admin_transferred`
//!    - Role events: `role_granted`, `role_revoked`, `role_parent_set`
//!    - Route events: `route_registered`, `route_updated`, `route_overwritten`
//!
//! ## Standard Event Topics
//!
//! Use these constants when publishing events to ensure consistency:

/// Standard event topic for admin transfer operations
pub const EVENT_ADMIN_TRANSFERRED: &str = "admin_transferred";

/// Standard event topic for route registration
pub const EVENT_ROUTE_REGISTERED: &str = "route_registered";

/// Standard event topic for route updates
pub const EVENT_ROUTE_UPDATED: &str = "route_updated";

/// Standard event topic for route overwrites
pub const EVENT_ROUTE_OVERWRITTEN: &str = "route_overwritten";

/// Standard event topic for paused route resolution attempts
pub const EVENT_ROUTE_RESOLVE_PAUSED: &str = "route_resolve_paused";

/// Standard event topic for successful routing
pub const EVENT_ROUTED: &str = "routed";

/// Standard event topic for route scoring
pub const EVENT_ROUTE_SCORED: &str = "route_scored";

/// Standard event topic for best route selection
pub const EVENT_BEST_ROUTE_SELECTED: &str = "best_route_selected";

/// Standard event topic for metadata updates
pub const EVENT_METADATA_UPDATED: &str = "metadata_updated";

/// Standard event topic for alias additions
pub const EVENT_ALIAS_ADDED: &str = "alias_added";

/// Standard event topic for alias removals
pub const EVENT_ALIAS_REMOVED: &str = "alias_removed";

/// Standard event topic for route removals
pub const EVENT_ROUTE_REMOVED: &str = "route_removed";

/// Standard event topic for route pause state changes
pub const EVENT_ROUTE_PAUSED: &str = "route_paused";

/// Standard event topic for router global pause state changes
pub const EVENT_ROUTER_PAUSED: &str = "router_paused";

/// Standard event topic for route tag additions
pub const EVENT_ROUTE_TAG_ADDED: &str = "route_tag_added";

/// Standard event topic for route tag removals
pub const EVENT_ROUTE_TAG_REMOVED: &str = "route_tag_removed";

/// Standard event topic for route TTL being set at registration
pub const EVENT_ROUTE_TTL_SET: &str = "route_ttl_set";

/// Standard event topic for route TTL extensions
pub const EVENT_ROUTE_TTL_EXTENDED: &str = "route_ttl_extended";

/// Standard event topic for resolution attempts on an expired route
pub const EVENT_ROUTE_RESOLVE_EXPIRED: &str = "route_resolve_expired";

/// Standard event topic for role grants
pub const EVENT_ROLE_GRANTED: &str = "role_granted";

/// Standard event topic for role revocations
pub const EVENT_ROLE_REVOKED: &str = "role_revoked";

/// Standard event topic for role parent assignments
pub const EVENT_ROLE_PARENT_SET: &str = "role_parent_set";

/// Standard event topic for role parent removals
pub const EVENT_ROLE_PARENT_REMOVED: &str = "role_parent_removed";

/// Standard event topic for address blacklisting
pub const EVENT_ADDRESS_BLACKLISTED: &str = "address_blacklisted";

/// Standard event topic for execution results
pub const EVENT_EXECUTION_RESULT: &str = "execution_result";

/// Standard event topic for execution retries
pub const EVENT_EXECUTION_RETRY: &str = "execution_retry";

/// Standard event topic for execution errors
pub const EVENT_EXECUTION_ERROR: &str = "execution_error";

/// Standard event topic for simulation results
pub const EVENT_SIMULATION_RESULT: &str = "simulation_result";

/// Standard event topic for fee estimations
pub const EVENT_FEE_ESTIMATED: &str = "fee_estimated";

/// Standard event topic for quote generation
pub const EVENT_QUOTE_GENERATED: &str = "quote_generated";

/// Standard event topic for pre-call middleware hooks
pub const EVENT_PRE_CALL: &str = "pre_call";

/// Standard event topic for post-call middleware hooks
pub const EVENT_POST_CALL: &str = "post_call";

/// Standard event topic for circuit breaker opening
pub const EVENT_CIRCUIT_OPENED: &str = "circuit_opened";

/// Standard event topic for circuit breaker closing
pub const EVENT_CIRCUIT_CLOSED: &str = "circuit_closed";

/// Standard event topic for middleware enable/disable
pub const EVENT_MIDDLEWARE_ENABLED: &str = "middleware_enabled";

/// Standard event topic for rate limit throttling
pub const EVENT_RATE_LIMIT_THROTTLED: &str = "rate_limit_throttled";

/// Standard event topic for rate limit exceeded
pub const EVENT_RATE_LIMIT_EXCEEDED: &str = "rate_limit_exceeded";

/// Standard event topic for call log clearing
pub const EVENT_CALL_LOG_CLEARED: &str = "call_log_cleared";

/// Standard event topic for multicall results
pub const EVENT_CALL_RESULT: &str = "call_result";

/// Standard event topic for a required call failure in multicall (includes index + contract context)
pub const EVENT_CALL_FAILED: &str = "call_failed";

/// Standard event topic for batch execution completion
pub const EVENT_BATCH_EXECUTED: &str = "batch_executed";

/// Standard event topic for max batch size updates
pub const EVENT_MAX_BATCH_SIZE_UPDATED: &str = "max_batch_size_updated";

/// Standard event topic for timelock operation queueing
pub const EVENT_OP_QUEUED: &str = "op_queued";

/// Standard event topic for timelock operation execution
pub const EVENT_OP_EXECUTED: &str = "op_executed";

/// Standard event topic for timelock operation cancellation
pub const EVENT_OP_CANCELLED: &str = "op_cancelled";

/// Standard event topic for timelock operation description updates
pub const EVENT_OP_DESCRIPTION_UPDATED: &str = "op_description_updated";

/// Standard event topic for timelock minimum delay updates
pub const EVENT_MIN_DELAY_UPDATED: &str = "min_delay_updated";

/// Standard event topic for contract registration in registry
pub const EVENT_CONTRACT_REGISTERED: &str = "contract_registered";

/// Standard event topic for contract deprecation in registry
pub const EVENT_CONTRACT_DEPRECATED: &str = "contract_deprecated";

// ── Batch types ───────────────────────────────────────────────────────────────

use soroban_sdk::{contracttype, Address, Env, IntoVal, String, Symbol, Val, Vec};

/// Per-call result payload used by multicall batch operations.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CallResult {
    pub target: Address,
    pub function: Symbol,
    pub success: bool,
}

/// Indexed success entry for void batch operations.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchSuccess {
    pub index: u32,
}

/// Indexed success entry for call batch operations.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchCallSuccess {
    pub index: u32,
    pub result: CallResult,
}

/// Indexed failure entry for batch operations.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchFailure {
    pub index: u32,
    pub message: String,
}

/// Standardized per-index batch operation result for void operations (`T = BatchUnit`).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchResult {
    pub successes: Vec<BatchSuccess>,
    pub failures: Vec<BatchFailure>,
}

/// Standardized per-index batch operation result for call operations (`T = CallResult`).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchCallResult {
    pub successes: Vec<BatchCallSuccess>,
    pub failures: Vec<BatchFailure>,
}

impl BatchResult {
    pub fn new(env: &Env) -> Self {
        Self {
            successes: Vec::new(env),
            failures: Vec::new(env),
        }
    }

    pub fn record_success(&mut self, index: u32) {
        self.successes.push_back(BatchSuccess { index });
    }

    pub fn record_failure(&mut self, env: &Env, index: u32, message: &str) {
        self.failures.push_back(BatchFailure {
            index,
            message: String::from_str(env, message),
        });
    }

    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }
}

impl BatchCallResult {
    pub fn new(env: &Env) -> Self {
        Self {
            successes: Vec::new(env),
            failures: Vec::new(env),
        }
    }

    pub fn record_success(&mut self, index: u32, value: CallResult) {
        self.successes.push_back(BatchCallSuccess {
            index,
            result: value,
        });
    }

    pub fn record_failure(&mut self, env: &Env, index: u32, message: &str) {
        self.failures.push_back(BatchFailure {
            index,
            message: String::from_str(env, message),
        });
    }

    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }
}

/// Checks that `caller` matches the admin address stored under `key`.
///
/// Expands to an expression that returns `Err($not_init_err)` if the key is
/// absent, or `Err($unauth_err)` if the caller does not match.
///
/// # Arguments
/// * `$env`          — `&Env` reference
/// * `$caller`       — `&Address` to validate
/// * `$key`          — storage key whose value is the admin `Address`
/// * `$not_init_err` — error variant returned when the key is missing
/// * `$unauth_err`   — error variant returned when the caller is not the admin
///
/// # Example
///
/// ```ignore
/// // Inside a #[contractimpl] block:
/// require_admin!(&env, &caller, &DataKey::Admin, MyError::NotInitialized, MyError::Unauthorized)?;
/// ```
#[macro_export]
macro_rules! require_admin {
    ($env:expr, $caller:expr, $key:expr, $not_init_err:expr, $unauth_err:expr) => {{
        let admin: soroban_sdk::Address =
            $env.storage().instance().get($key).ok_or($not_init_err)?;
        if &admin != $caller {
            return Err($unauth_err);
        }
        Ok::<(), _>(())
    }};
}

/// Convenience version when using DataKey::Admin and standard error variants.
///
/// This eliminates the repetitive `require_admin` / `require_super_admin` boilerplate
/// across all router contracts while allowing each contract to use its own error enum.
#[macro_export]
macro_rules! require_admin_simple {
    ($env:expr, $caller:expr, $data_key:expr, $error_type:ty) => {
        $crate::require_admin!(
            $env,
            $caller,
            $data_key,
            <$error_type>::NotInitialized,
            <$error_type>::Unauthorized
        )
    };
}

/// Returns `true` if `s` is empty or consists entirely of ASCII whitespace
/// (space 0x20, tab 0x09, newline 0x0A, vertical tab 0x0B, form feed 0x0C,
/// carriage return 0x0D).
///
/// # Example
///
/// ```
/// use router_common::is_whitespace_only;
/// assert!(is_whitespace_only(""));
/// assert!(is_whitespace_only("   "));
/// assert!(is_whitespace_only("\t\n\r"));
/// assert!(!is_whitespace_only("oracle"));
/// assert!(!is_whitespace_only(" oracle "));
/// ```
pub fn is_whitespace_only(s: &str) -> bool {
    s.is_empty() || s.bytes().all(|b| matches!(b, 9 | 10 | 11 | 12 | 13 | 32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_string_is_whitespace_only() {
        assert!(is_whitespace_only(""));
    }

    #[test]
    fn test_spaces_are_whitespace_only() {
        assert!(is_whitespace_only("   "));
    }

    #[test]
    fn test_tab_is_whitespace_only() {
        assert!(is_whitespace_only("\t"));
    }

    #[test]
    fn test_newline_is_whitespace_only() {
        assert!(is_whitespace_only("\n"));
    }

    #[test]
    fn test_carriage_return_is_whitespace_only() {
        assert!(is_whitespace_only("\r"));
    }

    #[test]
    fn test_mixed_whitespace_is_whitespace_only() {
        assert!(is_whitespace_only(" \t\n\r\x0b\x0c"));
    }

    #[test]
    fn test_normal_name_is_not_whitespace_only() {
        assert!(!is_whitespace_only("oracle"));
    }

    #[test]
    fn test_name_with_surrounding_spaces_is_not_whitespace_only() {
        assert!(!is_whitespace_only(" oracle "));
    }
}

/// Helper macro for completing the admin transfer after validation.
///
/// Use this in your transfer_admin function after you've already:
/// - Called current.require_auth()
/// - Called your own require_admin check
///
/// This macro:
/// - Sets the new admin in storage
/// - Publishes the admin_transferred event using the standard event topic
///
/// # Arguments
/// * `$env` - The Soroban environment reference
/// * `$current` - The current admin address (Address)
/// * `$new_admin` - The new admin address (Address)
/// * `$data_key_expr` - Expression for the storage key containing admin (e.g., &DataKey::Admin)
///
/// # Example
/// ```ignore
/// pub fn transfer_admin(
///     env: Env,
///     current: Address,
///     new_admin: Address,
/// ) -> Result<(), MyError> {
///     current.require_auth();
///     router_common::require_admin_simple!(&env, &current, &DataKey::Admin, MyError)?;
///     router_common::admin_transfer_complete!(&env, &current, &new_admin, &DataKey::Admin);
///     Ok(())
/// }
/// ```
#[macro_export]
macro_rules! admin_transfer_complete {
    ($env:expr, $current:expr, $new_admin:expr, $data_key_expr:expr) => {{
        $env.storage().instance().set($data_key_expr, $new_admin);
        $env.events().publish(
            (soroban_sdk::Symbol::new(
                $env,
                $crate::EVENT_ADMIN_TRANSFERRED,
            ),),
            ($current, $new_admin),
        );
    }};
}

// ── Shared storage keys & helpers ───────────────────────────────────────────────

/// Shared storage key for the admin address.
///
/// Each contract still owns its contract-specific `DataKey` enum, but the common
/// `Admin` key is consolidated here so the admin get/set/initialization helpers
/// below can be reused without redefining the variant in every contract.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum CommonDataKey {
    /// Address of the contract administrator.
    Admin,
}

/// Reads the admin address stored under `key`, or `None` if it has not been set.
///
/// `key` is generic so callers can pass their own `DataKey::Admin`, the shared
/// [`CommonDataKey::Admin`], or any equivalent storage key.
pub fn get_admin<K>(env: &Env, key: &K) -> Option<Address>
where
    K: IntoVal<Env, Val>,
{
    env.storage().instance().get(key)
}

/// Stores `admin` as the admin address under `key`.
pub fn set_admin<K>(env: &Env, key: &K, admin: &Address)
where
    K: IntoVal<Env, Val>,
{
    env.storage().instance().set(key, admin);
}

/// Returns `true` if a value is present under `key` (used as the initialization guard).
pub fn is_initialized<K>(env: &Env, key: &K) -> bool
where
    K: IntoVal<Env, Val>,
{
    env.storage().instance().has(key)
}

/// Returns `Ok(())` when a value exists under `key`, otherwise `Err(not_init_err)`.
///
/// Use this to guard operations that require the contract to already be initialized.
pub fn require_initialized<K, E>(env: &Env, key: &K, not_init_err: E) -> Result<(), E>
where
    K: IntoVal<Env, Val>,
{
    if env.storage().instance().has(key) {
        Ok(())
    } else {
        Err(not_init_err)
    }
}

/// Returns `Ok(())` when no value exists under `key`, otherwise `Err(already_init_err)`.
///
/// Use this in `initialize` to reject double initialization.
pub fn require_uninitialized<K, E>(env: &Env, key: &K, already_init_err: E) -> Result<(), E>
where
    K: IntoVal<Env, Val>,
{
    if env.storage().instance().has(key) {
        Err(already_init_err)
    } else {
        Ok(())
    }
}

/// Extends the time-to-live of the contract's instance storage.
///
/// Thin wrapper over `env.storage().instance().extend_ttl` so the common
/// `(threshold, extend_to)` pattern is expressed once.
pub fn extend_instance_ttl(env: &Env, threshold: u32, extend_to: u32) {
    env.storage().instance().extend_ttl(threshold, extend_to);
}

/// Default implementations for the admin and initialization storage patterns
/// shared across router contracts.
///
/// A contract opts in by implementing the three associated items — the admin
/// storage key plus its two initialization-related error variants — and gains
/// the admin get/set and initialization-guard helpers for free.
///
/// # Example
///
/// ```ignore
/// struct MyContractStorage;
///
/// impl router_common::StorageHelper for MyContractStorage {
///     type Key = DataKey;
///     type Error = MyError;
///
///     fn admin_key(_env: &Env) -> DataKey { DataKey::Admin }
///     fn not_initialized_error() -> MyError { MyError::NotInitialized }
///     fn already_initialized_error() -> MyError { MyError::AlreadyInitialized }
/// }
///
/// // later:
/// MyContractStorage::require_uninitialized(&env)?;
/// MyContractStorage::set_admin(&env, &admin);
/// ```
pub trait StorageHelper {
    /// Storage key type for this contract (typically its own `DataKey`).
    type Key: IntoVal<Env, Val>;
    /// Error type returned by the initialization guard helpers.
    type Error;

    /// The storage key under which the admin address is stored.
    fn admin_key(env: &Env) -> Self::Key;
    /// Error variant returned when the contract has not been initialized.
    fn not_initialized_error() -> Self::Error;
    /// Error variant returned when the contract is already initialized.
    fn already_initialized_error() -> Self::Error;

    /// Reads the admin address, or `None` if unset.
    fn get_admin(env: &Env) -> Option<Address> {
        crate::get_admin(env, &Self::admin_key(env))
    }

    /// Stores `admin` as the admin address.
    fn set_admin(env: &Env, admin: &Address) {
        crate::set_admin(env, &Self::admin_key(env), admin);
    }

    /// Returns `true` if the admin address is set.
    fn is_initialized(env: &Env) -> bool {
        crate::is_initialized(env, &Self::admin_key(env))
    }

    /// Errors with `not_initialized_error()` unless the contract is initialized.
    fn require_initialized(env: &Env) -> Result<(), Self::Error> {
        crate::require_initialized(env, &Self::admin_key(env), Self::not_initialized_error())
    }

    /// Errors with `already_initialized_error()` if the contract is already initialized.
    fn require_uninitialized(env: &Env) -> Result<(), Self::Error> {
        crate::require_uninitialized(
            env,
            &Self::admin_key(env),
            Self::already_initialized_error(),
        )
    }
}

#[cfg(test)]
mod storage_helper_tests {
    use super::*;
    use soroban_sdk::{contract, contracterror, testutils::Address as _};

    #[contract]
    struct TestContract;

    #[contracterror]
    #[derive(Copy, Clone, Debug, PartialEq)]
    enum TestError {
        NotInitialized = 1,
        AlreadyInitialized = 2,
    }

    struct TestStorage;

    impl StorageHelper for TestStorage {
        type Key = CommonDataKey;
        type Error = TestError;

        fn admin_key(_env: &Env) -> CommonDataKey {
            CommonDataKey::Admin
        }
        fn not_initialized_error() -> TestError {
            TestError::NotInitialized
        }
        fn already_initialized_error() -> TestError {
            TestError::AlreadyInitialized
        }
    }

    #[test]
    fn admin_round_trips_through_helpers() {
        let env = Env::default();
        let id = env.register_contract(None, TestContract);
        env.as_contract(&id, || {
            let admin = Address::generate(&env);
            assert!(get_admin(&env, &CommonDataKey::Admin).is_none());
            set_admin(&env, &CommonDataKey::Admin, &admin);
            assert_eq!(get_admin(&env, &CommonDataKey::Admin), Some(admin));
        });
    }

    #[test]
    fn initialization_guards_behave() {
        let env = Env::default();
        let id = env.register_contract(None, TestContract);
        env.as_contract(&id, || {
            assert!(!is_initialized(&env, &CommonDataKey::Admin));
            assert_eq!(
                require_initialized(&env, &CommonDataKey::Admin, TestError::NotInitialized),
                Err(TestError::NotInitialized)
            );
            assert_eq!(
                require_uninitialized(&env, &CommonDataKey::Admin, TestError::AlreadyInitialized),
                Ok(())
            );

            set_admin(&env, &CommonDataKey::Admin, &Address::generate(&env));

            assert!(is_initialized(&env, &CommonDataKey::Admin));
            assert_eq!(
                require_initialized(&env, &CommonDataKey::Admin, TestError::NotInitialized),
                Ok(())
            );
            assert_eq!(
                require_uninitialized(&env, &CommonDataKey::Admin, TestError::AlreadyInitialized),
                Err(TestError::AlreadyInitialized)
            );
        });
    }

    #[test]
    fn trait_defaults_delegate_to_free_functions() {
        let env = Env::default();
        let id = env.register_contract(None, TestContract);
        env.as_contract(&id, || {
            assert!(!TestStorage::is_initialized(&env));
            assert_eq!(TestStorage::require_uninitialized(&env), Ok(()));

            let admin = Address::generate(&env);
            TestStorage::set_admin(&env, &admin);

            assert_eq!(TestStorage::get_admin(&env), Some(admin));
            assert!(TestStorage::is_initialized(&env));
            assert_eq!(TestStorage::require_initialized(&env), Ok(()));
            assert_eq!(
                TestStorage::require_uninitialized(&env),
                Err(TestError::AlreadyInitialized)
            );
        });
    }

    #[test]
    fn extend_instance_ttl_does_not_panic() {
        let env = Env::default();
        let id = env.register_contract(None, TestContract);
        env.as_contract(&id, || {
            set_admin(&env, &CommonDataKey::Admin, &Address::generate(&env));
            extend_instance_ttl(&env, 100, 1000);
        });
    }
}
