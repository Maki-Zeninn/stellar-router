// Batch-60: Documentation and Performance Fixes for Stellar Router
// Issues: #1268, #1267, #1269, #1270

// ────────────────────────────────────────────────────────────────────────────
// #1268: Document grant_role_batch parameters and return type
// ────────────────────────────────────────────────────────────────────────────

/**
 * BEFORE (contracts/router-access/src/lib.rs:138-162):
 *
 * pub fn grant_role_batch(
 *     env: Env,
 *     admin: Address,
 *     accounts: Vec<Address>,
 *     role: String,
 *     expires_in: Option<u64>,
 *     fail_fast: bool,
 * ) -> Result<router_common::BatchResult, AccessError> {
 *
 * AFTER: Replace with improved doc comment:
 */

pub const GRANT_ROLE_BATCH_DOCS: &str = r#"
    /// Grant a role to multiple accounts in one call, returning per-account results.
    ///
    /// This function processes a batch of accounts and attempts to grant the specified role
    /// to each. Authorization (require_role_manager) is checked once up-front on behalf of
    /// the `admin`; per-account failures are collected, not fatal.
    ///
    /// # Parameters
    ///
    /// * `admin` — Address making the request; must hold the role-manager privilege for `role`.
    ///   Authorization is checked once at entry; the request fails with `AccessError::Unauthorized`
    ///   if `admin` does not have the privilege.
    /// * `accounts` — Vector of target addresses to grant the role to. Order is preserved in
    ///   results; if `accounts` is empty, returns an empty-success BatchResult.
    /// * `role` — String name of the role to grant. Must already exist in the system
    ///   (introduced via assign_role or grant_role).
    /// * `expires_in` — Optional expiry duration (seconds from now). Pass `None` for
    ///   a permanent grant (no expiry). Pass `Some(n)` to automatically revoke the role
    ///   after `n` seconds.
    /// * `fail_fast` — Boolean controlling batch semantics on per-account failure:
    ///   - `true` — stop processing accounts as soon as any account fails. Earlier successful
    ///     grants remain persisted; results will include successes followed by zero or more
    ///     failures depending on when processing stopped.
    ///   - `false` — continue processing all accounts regardless of per-account failures.
    ///     Results will reflect success/failure for every account in the original order.
    ///
    /// # Return Value
    ///
    /// `Result<router_common::BatchResult, AccessError>`
    ///
    /// **Error case (Err):** Reserved for fatal authorization failure (require_role_manager):
    /// - Returns `AccessError::Unauthorized` if `admin` does not hold the role-manager
    ///   privilege for `role`.
    ///
    /// **Success case (Ok(BatchResult)):** Always returned if authorization succeeds, even
    /// if all per-account grants fail. The `BatchResult` contains:
    /// - `successes` (Vec<u32>) — indices (0..accounts.len()) of successfully granted accounts.
    /// - `failures` (Vec<BatchItemError>) — indices and errors for accounts that failed.
    ///   Each `BatchItemError` contains an index and an `error` field mapping via
    ///   `access_error_to_batch`. Common per-account failures:
    ///   - `BatchItemError { index: i, error: AccessError::AlreadyHasRole }` — target
    ///     already holds an active grant for `role`.
    ///   - `BatchItemError { index: i, error: AccessError::MaxGrantsPerRoleExceeded }` —
    ///     role has reached its per-role member limit.
    ///   - `BatchItemError { index: i, error: AccessError::Blacklisted }` — target
    ///     is blacklisted and cannot receive roles.
    ///
    /// # Behavior
    ///
    /// - If `fail_fast` is true and account[2] fails, results include successes for [0,1]
    ///   and failures for [2]. Accounts [3..] are not processed.
    /// - Storage writes are not atomic at the batch level; partially committed results persist
    ///   even if fail_fast stops early or later accounts fail.
    /// - All per-account errors are mapped via `access_error_to_batch`, converting AccessError
    ///   variants (Blacklisted, AlreadyHasRole, etc.) into BatchItemError enum variants.
    ///   Callers must match the error type to interpret the failure reason.
"#;

// ────────────────────────────────────────────────────────────────────────────
// #1267: Reorder blacklist check before deactivate_role_grant
// ────────────────────────────────────────────────────────────────────────────

/**
 * BEFORE (contracts/router-access/src/lib.rs:499-505):
 *
 *     // Remove grant from source (including member counters/lists).
 *     Self::deactivate_role_grant(&env, &role, &from);
 *
 *     // Grant to destination with same expiry timestamp.
 *     if Self::is_blacklisted_internal(&env, &to) {
 *         return Err(AccessError::Blacklisted);
 *     }
 *
 * AFTER: Swap lines so blacklist check happens first
 */

pub const TRANSFER_ROLE_MEMBERSHIP_FIX: &str = r#"
    // Check blacklist before any storage mutations (cheaper, has no side effects).
    if Self::is_blacklisted_internal(&env, &to) {
        return Err(AccessError::Blacklisted);
    }

    // Remove grant from source (including member counters/lists).
    Self::deactivate_role_grant(&env, &role, &from);

    // Grant to destination with same expiry timestamp.
"#;

// ────────────────────────────────────────────────────────────────────────────
// #1269: Add extend_instance_ttl calls to router-middleware
// ────────────────────────────────────────────────────────────────────────────

/**
 * The router-middleware contract writes instance storage at these sites:
 * - initialize (line 246-248): Admin, GlobalEnabled, TotalCalls
 * - set_global_enabled (line 655+): GlobalEnabled
 * - set_rate_limit_strategy (line 986+): route config
 *
 * After each, add:
 * router_common::extend_instance_ttl(&env, router_common::INSTANCE_TTL_THRESHOLD, router_common::INSTANCE_TTL_EXTEND_TO);
 *
 * Example fix at initialize:
 */

pub const EXTEND_INSTANCE_TTL_FIX: &str = r#"
    pub fn initialize(env: Env, admin: Address) -> Result<(), MiddlewareError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(MiddlewareError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::GlobalEnabled, &true);
        env.storage().instance().set(&DataKey::TotalCalls, &0u64);

        // Extend instance TTL to prevent archival
        router_common::extend_instance_ttl(
            &env,
            router_common::INSTANCE_TTL_THRESHOLD,
            router_common::INSTANCE_TTL_EXTEND_TO,
        );

        Ok(())
    }

    pub fn set_global_enabled(
        env: Env,
        caller: Address,
        enabled: bool,
    ) -> Result<(), MiddlewareError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        env.storage().instance().set(&DataKey::GlobalEnabled, &enabled);

        // Extend instance TTL to prevent archival
        router_common::extend_instance_ttl(
            &env,
            router_common::INSTANCE_TTL_THRESHOLD,
            router_common::INSTANCE_TTL_EXTEND_TO,
        );

        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_MIDDLEWARE_ENABLED),),
            enabled,
        );
        Ok(())
    }

    pub fn set_rate_limit_strategy(
        env: Env,
        caller: Address,
        route_name: String,
        rate_limit: Option<RateLimitConfig>,
    ) -> Result<(), MiddlewareError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        match rate_limit {
            Some(config) => {
                env.storage()
                    .instance()
                    .set(&DataKey::RateLimitConfig(route_name.clone()), &config);
            }
            None => {
                env.storage()
                    .instance()
                    .remove(&DataKey::RateLimitConfig(route_name.clone()));
            }
        }

        // Extend instance TTL to prevent archival
        router_common::extend_instance_ttl(
            &env,
            router_common::INSTANCE_TTL_THRESHOLD,
            router_common::INSTANCE_TTL_EXTEND_TO,
        );

        Ok(())
    }
"#;

// ────────────────────────────────────────────────────────────────────────────
// #1270: Update EVENT_NAMING_CONVENTION.md router-core section
// ────────────────────────────────────────────────────────────────────────────

/**
 * BEFORE (contracts/router-common/EVENT_NAMING_CONVENTION.md:29-34):
 *
 * ### router-core
 * - `route_registered` — (route_name, address)
 * - `routed` — (route_name, address)
 * - `alias_added` — (existing_name, alias_name)
 * - `metadata_updated` — (route_name, metadata)
 * - `admin_transferred` — (old_admin, new_admin)
 *
 * AFTER: Replace with complete list of all 20 events
 */

pub const EVENT_NAMING_CONVENTION_ROUTER_CORE_UPDATE: &str = r#"
### router-core
- `route_registered` — (route_name, address)
- `routed` — (route_name, address)
- `alias_added` — (existing_name, alias_name)
- `alias_removed` — (alias_name, original_name)
- `alias_resolved` — (alias, resolved_name)
- `metadata_updated` — (route_name, metadata)
- `best_route_selected` — (selected_route, source_route)
- `router_paused` — (enabled)
- `route_overwritten` — (route_name, old_address, new_address)
- `route_paused` — (route_name, paused)
- `route_removed` — (route_name)
- `route_resolve_expired` — (route_name, expiry_time)
- `route_resolve_paused` — (route_name, paused)
- `route_scored` — (route_name, score)
- `route_tag_added` — (route_name, tag)
- `route_tag_removed` — (route_name, tag)
- `route_ttl_extended` — (route_name, new_ttl)
- `route_ttl_set` — (route_name, ttl)
- `route_updated` — (route_name, new_address)
- `admin_transferred` — (old_admin, new_admin)
"#;
