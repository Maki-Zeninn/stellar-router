// Batch-61: Event Documentation Fixes and Unused Code Removal
// Issues: #1271, #1272, #1273, #1274

// ────────────────────────────────────────────────────────────────────────────
// #1271: Fix router-access section in EVENT_NAMING_CONVENTION.md
// ────────────────────────────────────────────────────────────────────────────

/**
 * CHANGES:
 * - Remove nonexistent `role_parent_removed` event (never emitted, constant was removed)
 * - Add missing `role_limits_set` event (EVENT_ROLE_LIMITS_SET, actually emitted)
 *
 * Actual events in router-access:
 * - role_granted
 * - role_revoked
 * - role_parent_set
 * - role_admin_set
 * - address_blacklisted
 * - address_unblacklisted
 * - role_expired
 * - role_limits_set (missing from doc)
 * - admin_transferred
 */

pub const ROUTER_ACCESS_EVENTS_CORRECTED: &str = r#"
### router-access
- `role_granted` — (role, target, expiry_timestamp)
- `role_revoked` — (role, target)
- `role_parent_set` — (role, parent_role)
- `role_admin_set` — (role, admin)
- `role_limits_set` — (max_roles, max_grants_per_role)
- `address_blacklisted` — (address)
- `address_unblacklisted` — (address)
- `role_expired` — (role, target)
- `admin_transferred` — (old_admin, new_admin)
"#;

// ────────────────────────────────────────────────────────────────────────────
// #1272: Fix router-multicall section in EVENT_NAMING_CONVENTION.md
// ────────────────────────────────────────────────────────────────────────────

/**
 * CHANGES:
 * - Add missing `initialized` event (EVENT_INITIALIZED, actually emitted)
 * - Add missing `call_failed` event (EVENT_CALL_FAILED, actually emitted)
 *
 * Actual events in router-multicall:
 * - call_result
 * - call_failed (missing from doc)
 * - batch_executed
 * - max_batch_size_updated
 * - initialized (missing from doc)
 * - admin_transferred
 */

pub const ROUTER_MULTICALL_EVENTS_CORRECTED: &str = r#"
### router-multicall
- `call_result` — (caller, target, function, success)
- `call_failed` — (caller, target, function, error)
- `batch_executed` — (summary data)
- `max_batch_size_updated` — (old_size, new_size)
- `initialized` — (admin, max_batch_size)
- `admin_transferred` — (old_admin, new_admin)
"#;

// ────────────────────────────────────────────────────────────────────────────
// #1273: Fix router-execution section in EVENT_NAMING_CONVENTION.md
// ────────────────────────────────────────────────────────────────────────────

/**
 * CHANGES:
 * - Add missing `admin_transferred` event (EVENT_ADMIN_TRANSFERRED, actually emitted)
 * - Add missing `execution_error` event (EVENT_EXECUTION_ERROR, actually emitted)
 * - Add missing `execution_retry` event (EVENT_EXECUTION_RETRY, actually emitted)
 *
 * Actual events in router-execution:
 * - execution_result
 * - execution_error (missing from doc)
 * - execution_retry (missing from doc)
 * - fee_estimated
 * - simulation_result
 * - backoff_config_updated (not mentioned in issue but emitted)
 * - admin_transferred (missing from doc)
 */

pub const ROUTER_EXECUTION_EVENTS_CORRECTED: &str = r#"
### router-execution
- `execution_result` — (target, function, success, attempts)
- `execution_error` — (target, function, error)
- `execution_retry` — (target, function, retry_count)
- `fee_estimated` — (total_fee, surge_pricing)
- `simulation_result` — (target, function, success)
- `backoff_config_updated` — (new_backoff_config)
- `admin_transferred` — (old_admin, new_admin)
"#;

// ────────────────────────────────────────────────────────────────────────────
// #1274: Remove unused StorageHelper trait and helper functions
// ────────────────────────────────────────────────────────────────────────────

/**
 * REMOVAL LOCATIONS in contracts/router-common/src/lib.rs:
 *
 * 1. Remove module documentation references (lines 17-20):
 *    - Remove references to get_admin, set_admin, is_initialized, etc.
 *    - Remove StorageHelper documentation
 *
 * 2. Remove free functions (lines 823-866):
 *    - pub fn get_admin<K>(env: &Env, key: &K) -> Option<Address>
 *    - pub fn set_admin<K>(env: &Env, key: &K, admin: &Address)
 *    - pub fn is_initialized<K>(env: &Env, key: &K) -> bool
 *    - pub fn require_initialized<K, E>(env: &Env, key: &K, not_init_err: E) -> Result<(), E>
 *    - pub fn require_uninitialized<K, E>(env: &Env, key: &K, already_init_err: E) -> Result<(), E>
 *
 * 3. Remove StorageHelper trait (lines 894-945):
 *    - Entire trait definition, doc comments, and default implementations
 *
 * 4. Keep test module but remove StorageHelper implementation example (line 969+)
 *    - Or remove entire test module if it only tests StorageHelper
 *
 * Impact: No contracts depend on these; all hand-roll their own initialization patterns.
 */

pub const STORAGE_HELPER_REMOVAL_NOTE: &str = r#"
The StorageHelper trait and its helper functions (get_admin, set_admin, is_initialized,
require_initialized, require_uninitialized) are completely unused by all 8 dependent contracts.
Each contract hand-rolls its own initialization pattern, so the trait provides no value.

Removal is safe and reduces code maintainability burden.
"#;
