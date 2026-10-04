#![no_std]

//! # router-timelock
//!
//! Delayed execution queue for sensitive router configuration changes.
//! Operations must wait a configurable minimum delay before execution.
//! Operations can be cancelled before execution.
//! Operations expire if not executed within `eta + grace_period_seconds`.
//!
//! ## Events (following naming convention: past tense verbs in snake_case)
//! - `op_queued`              — Operation queued (op_id, target, eta, grace_period_seconds)
//! - `op_executed`            — Operation executed (op_id, target)
//! - `op_cancelled`           — Operation cancelled (op_id)
//! - `op_description_updated` — Operation description updated (op_id, new_description)
//! - `min_delay_updated`      — Minimum delay updated (old_min_delay, new_min_delay)
//! - `ops_cleaned`            — Expired/finalized operations cleaned (count)
//! - `admin_transferred`      — Admin transferred (old_admin, new_admin)

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, xdr::ToXdr, Address, Bytes, Env, String,
    Symbol, Vec,
};

/// Complete storage-key map for the router-timelock contract.
#[contracttype]
pub enum DataKey {
    /// The admin address authorized to manage timelock operations.
    Admin,
    /// The minimum delay (in seconds) enforced before an operation can be executed.
    MinDelay,
    /// `op_id -> Op`: maps an operation id to its pending operation.
    Op(Bytes),
    /// `Vec<Bytes>`: IDs of ops that are neither executed nor cancelled.
    PendingOps,
    /// `u32`: maximum allowed number of pending operations.
    MaxPendingOps,
    /// `op_id -> Vec<Bytes>`: dependency operation IDs for a given op.
    Deps(Bytes),
    /// `Vec<Bytes>`: IDs of executed or cancelled ops whose `Op`/`Deps`
    /// storage has not yet been reclaimed by `cleanup_expired`.
    FinalizedOps,
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    /// Address that queued this operation via `queue`.
    pub proposer: Address,
    /// Human-readable description of the operation; can be updated via `update_description`.
    pub description: String,
    /// Address the operation applies to.
    pub target: Address,
    /// Unix timestamp (ledger time) after which the operation becomes executable.
    pub eta: u64,
    /// Seconds after `eta` during which the operation may be executed.
    /// After `eta + grace_period_seconds` the operation is considered expired
    /// and can no longer be executed.
    pub grace_period_seconds: u64,
    /// Whether `execute` has successfully run for this operation.
    pub executed: bool,
    /// Whether `cancel` was called on this operation before execution.
    pub cancelled: bool,
}

/// Human-readable status of a timelock operation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum OperationStatus {
    /// Queued and waiting for ETA to elapse.
    Queued,
    /// ETA has elapsed, still within grace period, not yet executed.
    Ready,
    /// Successfully executed.
    Executed,
    /// Cancelled before execution.
    Cancelled,
    /// Grace period has elapsed without execution; operation can no longer be executed.
    Expired,
    /// One or more dependencies were cancelled or expired without executing;
    /// this operation can never execute.
    Blocked,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TimelockError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    NotFound = 4,
    NotReady = 5,
    AlreadyExecuted = 6,
    Cancelled = 7,
    DelayTooShort = 8,
    /// The grace period has elapsed; the operation can no longer be executed.
    Expired = 9,
    /// The maximum number of pending operations has been reached.
    QueueFull = 10,
    /// A dependency references itself or creates a cycle.
    CircularDependency = 11,
    /// Dependency chain exceeds maximum allowed depth.
    DependencyTooDeep = 12,
    /// An operation with this exact (description, target, eta) already exists.
    AlreadyQueued = 13,
    /// A dependency of this operation has not yet been executed.
    DependencyNotExecuted = 14,
    /// `grace_period_seconds` exceeds [`RouterTimelock::MAX_GRACE_PERIOD_SECONDS`].
    ///
    /// Very large grace periods (e.g. `u64::MAX`) would cause arithmetic
    /// overflow in `eta + grace_period_seconds` and create operations that
    /// can never practically expire.
    GracePeriodTooLong = 15,
    /// `eta + grace_period_seconds` overflows `u64`.
    ///
    /// Returned by `execute` / status queries if a stored operation somehow
    /// has values that produce overflow (should not occur after the
    /// `GracePeriodTooLong` guard was introduced, but kept as a safety net).
    ExpiryOverflow = 16,
    /// `ledger_timestamp + delay` overflows `u64`.
    DelayTooLong = 17,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RouterTimelock;

#[contractimpl]
impl RouterTimelock {
    /// Maximum depth at which `check_dependency_depth` will recurse into
    /// ancestor operations.
    ///
    /// Because `check_dependency_depth` is first invoked with `depth = 0` for
    /// each *direct* dependency passed to `queue()`, a value of `N` here
    /// permits an ancestor chain of **`N + 1` nodes** before the call is
    /// rejected.  In other words:
    ///
    /// - A 9-node ancestor chain reaches depth 8 at its deepest point →
    ///   **allowed** (8 == `MAX_DEPENDENCY_DEPTH`).
    /// - A 10-node ancestor chain reaches depth 9 →
    ///   **rejected** (9 > `MAX_DEPENDENCY_DEPTH`).
    ///
    /// If you need to change this limit, remember that the effective maximum
    /// permitted chain length is `MAX_DEPENDENCY_DEPTH + 1`, not
    /// `MAX_DEPENDENCY_DEPTH`.
    const MAX_DEPENDENCY_DEPTH: u32 = 8;

    /// Maximum allowed `grace_period_seconds` when queueing an operation.
    ///
    /// Set to 30 days (2 592 000 seconds). This prevents:
    /// - Arithmetic overflow in `eta + grace_period_seconds` (a `u64::MAX`
    ///   value would wrap around and produce a nonsensical expiry).
    /// - Effectively permanent operations that can never expire in practice
    ///   (e.g. grace periods measured in centuries).
    ///
    /// Admins who genuinely need a longer window should re-queue the
    /// operation closer to its intended execution time.
    const MAX_GRACE_PERIOD_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days

    /// Minimum remaining TTL (in ledgers) before instance storage is extended.
    /// ~30 days at 5 s/ledger.
    const INSTANCE_TTL_THRESHOLD: u32 = 17280 * 30;

    /// Target TTL (in ledgers) applied to instance storage on every entry point.
    /// ~60 days at 5 s/ledger.
    const INSTANCE_TTL_EXTEND_TO: u32 = 17280 * 60;

    /// Initialize with an admin, minimum delay (seconds), and maximum pending operations limit.
    ///
    /// `max_pending_ops == 0` disables the pending-operations cap (unlimited).
    /// Note this differs from `router-core`'s `set_max_routes`, which rejects 0.
    pub fn initialize(
        env: Env,
        admin: Address,
        min_delay: u64,
        max_pending_ops: u32,
    ) -> Result<(), TimelockError> {
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(TimelockError::AlreadyInitialized);
        }
        if min_delay == 0 {
            return Err(TimelockError::DelayTooShort);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::MinDelay, &min_delay);
        env.storage()
            .instance()
            .set(&DataKey::MaxPendingOps, &max_pending_ops);
        Ok(())
    }

    /// Queue an operation. Returns the op_id (SHA-256 of description + target + eta).
    ///
    /// `grace_period_seconds` defines the window after `eta` during which the
    /// operation may be executed. Once `eta + grace_period_seconds` has elapsed
    /// the operation is considered expired and can no longer be executed.
    ///
    /// Emits `op_queued` with `(op_id, target, eta, grace_period_seconds)`.
    pub fn queue(
        env: Env,
        proposer: Address,
        description: String,
        target: Address,
        delay: u64,
        grace_period_seconds: u64,
        deps: Vec<Bytes>,
    ) -> Result<Bytes, TimelockError> {
        proposer.require_auth();
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        router_common::require_admin_simple!(&env, &proposer, &DataKey::Admin, TimelockError)?;

        let min_delay: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MinDelay)
            .ok_or(TimelockError::NotInitialized)?;

        // `initialize` and `set_min_delay` both reject a zero value before
        // storing `DataKey::MinDelay`, so a stored MinDelay is guaranteed
        // non-zero — the `min_delay == 0` disjunct that previously appeared
        // here was unreachable dead code (mirrors Issue #1207 cleanup in
        // `set_min_delay`).
        if delay < min_delay {
            return Err(TimelockError::DelayTooShort);
        }

        // Guard against overflow and unreasonably long grace periods.
        // A value like u64::MAX would wrap eta + grace_period_seconds and make
        // expiry checks behave unpredictably; a value measured in centuries
        // creates operations that can never expire in practice.
        if grace_period_seconds > Self::MAX_GRACE_PERIOD_SECONDS {
            return Err(TimelockError::GracePeriodTooLong);
        }

        let pending: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::PendingOps)
            .unwrap_or_else(|| Vec::new(&env));
        let max_pending: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MaxPendingOps)
            .unwrap_or(0);
        if max_pending > 0 && pending.len() >= max_pending {
            return Err(TimelockError::QueueFull);
        }

        let eta = env
            .ledger()
            .timestamp()
            .checked_add(delay)
            .ok_or(TimelockError::DelayTooLong)?;

        // Derive op_id from description bytes + target bytes + eta
        let mut preimage = Bytes::new(&env);
        preimage.append(&description.clone().to_xdr(&env));
        preimage.append(&target.clone().to_xdr(&env));
        let eta_bytes = eta.to_be_bytes();
        preimage.append(&Bytes::from_array(&env, &eta_bytes));

        let op_id: Bytes = env.crypto().sha256(&preimage).into();

        // Reject if an operation with this exact id (description + target + eta)
        // already exists, regardless of its current state — otherwise a
        // re-submission with matching parameters would silently overwrite an
        // already-executed/cancelled op back to pending.
        if env.storage().instance().has(&DataKey::Op(op_id.clone())) {
            return Err(TimelockError::AlreadyQueued);
        }

        // Validate dependencies: no circular references, dependency must exist, depth within limits
        for dep_id in deps.iter() {
            if dep_id == op_id {
                return Err(TimelockError::CircularDependency);
            }
            if !env.storage().instance().has(&DataKey::Op(dep_id.clone())) {
                return Err(TimelockError::NotFound);
            }
            Self::check_dependency_depth(&env, dep_id.clone(), 0)?;
        }

        let op = Op {
            proposer,
            description,
            target: target.clone(),
            eta,
            grace_period_seconds,
            executed: false,
            cancelled: false,
        };
        env.storage()
            .instance()
            .set(&DataKey::Op(op_id.clone()), &op);

        // Store dependencies for recursive depth checking
        if !deps.is_empty() {
            env.storage()
                .instance()
                .set(&DataKey::Deps(op_id.clone()), &deps);
        }

        // Track in pending ops index for efficient querying
        Self::add_to_pending_ops(&env, &op_id);

        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_OP_QUEUED),),
            (op_id.clone(), target, eta, grace_period_seconds),
        );

        Ok(op_id)
    }

    /// Cancel a queued operation before it is executed.
    pub fn cancel(env: Env, caller: Address, op_id: Bytes) -> Result<(), TimelockError> {
        caller.require_auth();
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, TimelockError)?;

        let mut op: Op = env
            .storage()
            .instance()
            .get(&DataKey::Op(op_id.clone()))
            .ok_or(TimelockError::NotFound)?;

        Self::require_op_pending(&op)?;

        op.cancelled = true;
        env.storage()
            .instance()
            .set(&DataKey::Op(op_id.clone()), &op);

        Self::remove_from_pending_ops(&env, &op_id);
        Self::add_to_finalized_ops(&env, &op_id);

        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_OP_CANCELLED),),
            op_id,
        );

        Ok(())
    }

    /// Execute a queued operation after its ETA has passed and before its grace period expires.
    ///
    /// Returns `TimelockError::NotReady` if called before `eta`.
    /// Returns `TimelockError::Expired` if called after `eta + grace_period_seconds`.
    pub fn execute(env: Env, caller: Address, op_id: Bytes) -> Result<(), TimelockError> {
        caller.require_auth();
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, TimelockError)?;

        let mut op: Op = env
            .storage()
            .instance()
            .get(&DataKey::Op(op_id.clone()))
            .ok_or(TimelockError::NotFound)?;

        if op.cancelled {
            return Err(TimelockError::Cancelled);
        }
        if op.executed {
            return Err(TimelockError::AlreadyExecuted);
        }

        let now = env.ledger().timestamp();
        if now < op.eta {
            return Err(TimelockError::NotReady);
        }
        // Use checked_add to guard against overflow on the expiry boundary.
        // In practice GracePeriodTooLong prevents overflow at queue time, but
        // checked_add is a defence-in-depth measure for any existing stored ops.
        let expiry = op
            .eta
            .checked_add(op.grace_period_seconds)
            .ok_or(TimelockError::ExpiryOverflow)?;
        if now > expiry {
            return Err(TimelockError::Expired);
        }

        Self::require_dependencies_executed(&env, &op_id)?;

        op.executed = true;
        env.storage()
            .instance()
            .set(&DataKey::Op(op_id.clone()), &op);

        Self::remove_from_pending_ops(&env, &op_id);
        Self::add_to_finalized_ops(&env, &op_id);

        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_OP_EXECUTED),),
            (op_id, op.target),
        );

        Ok(())
    }

    /// Update the description of a queued (not yet executed or cancelled) operation.
    ///
    /// Only the admin may call this. The operation must still be pending —
    /// descriptions of executed or cancelled operations cannot be changed.
    ///
    /// Emits `op_description_updated` with `(op_id, new_description)`.
    pub fn update_description(
        env: Env,
        caller: Address,
        op_id: Bytes,
        new_description: String,
    ) -> Result<(), TimelockError> {
        caller.require_auth();
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, TimelockError)?;

        let mut op: Op = env
            .storage()
            .instance()
            .get(&DataKey::Op(op_id.clone()))
            .ok_or(TimelockError::NotFound)?;

        if op.executed {
            return Err(TimelockError::AlreadyExecuted);
        }
        if op.cancelled {
            return Err(TimelockError::Cancelled);
        }

        op.description = new_description.clone();
        env.storage()
            .instance()
            .set(&DataKey::Op(op_id.clone()), &op);

        env.events().publish(
            (Symbol::new(
                &env,
                router_common::EVENT_OP_DESCRIPTION_UPDATED,
            ),),
            (op_id, new_description),
        );

        Ok(())
    }

    /// Reclaim storage for operations that can never execute again.
    ///
    /// Removes expired operations from the pending operations index, and frees
    /// the `Op`/`Deps` storage of expired, executed and cancelled operations.
    /// An operation that is still a dependency of a live (non-expired) pending
    /// operation is skipped, since removing it would change that operation's
    /// dependency checks; it is reclaimed on a later call once its dependents
    /// are finalized or expired.
    ///
    /// This permissionless function allows anyone to clean up stale state and help
    /// keep the contract within storage limits.
    ///
    /// # Arguments
    /// * `caller` - The caller of this function.
    /// * `limit` - The maximum number of operations to remove in this batch.
    ///
    /// # Returns
    /// The number of operations removed.
    pub fn cleanup_expired(env: Env, caller: Address, limit: u32) -> Result<u32, TimelockError> {
        caller.require_auth();
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );

        let pending: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::PendingOps)
            .unwrap_or_else(|| Vec::new(&env));
        let finalized: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::FinalizedOps)
            .unwrap_or_else(|| Vec::new(&env));

        let now = env.ledger().timestamp();

        // Dependencies of live pending ops must keep their storage, otherwise
        // `require_dependencies_executed` / `has_blocked_dependency` would see
        // them as missing.
        let mut referenced: Vec<Bytes> = Vec::new(&env);
        for op_id in pending.iter() {
            if let Some(op) = env
                .storage()
                .instance()
                .get::<DataKey, Op>(&DataKey::Op(op_id.clone()))
            {
                if !Self::is_expired(&op, now) {
                    let deps: Vec<Bytes> = env
                        .storage()
                        .instance()
                        .get(&DataKey::Deps(op_id))
                        .unwrap_or_else(|| Vec::new(&env));
                    referenced.append(&deps);
                }
            }
        }

        let mut cleaned_count = 0u32;
        let mut new_pending = Vec::new(&env);
        let mut new_finalized = Vec::new(&env);

        for op_id in pending.iter() {
            if cleaned_count >= limit {
                new_pending.push_back(op_id);
                continue;
            }

            if let Some(op) = env
                .storage()
                .instance()
                .get::<DataKey, Op>(&DataKey::Op(op_id.clone()))
            {
                // Overflow is treated as expired (i.e. clean it up) rather
                // than leaving the op stuck in the queue.
                if Self::is_expired(&op, now) && !referenced.contains(&op_id) {
                    Self::remove_op_storage(&env, &op_id);
                    cleaned_count += 1;
                } else {
                    new_pending.push_back(op_id);
                }
            } else {
                // If the operation data somehow doesn't exist, we just clean it up too
                cleaned_count += 1;
            }
        }

        for op_id in finalized.iter() {
            if cleaned_count >= limit || referenced.contains(&op_id) {
                new_finalized.push_back(op_id);
                continue;
            }
            Self::remove_op_storage(&env, &op_id);
            cleaned_count += 1;
        }

        if cleaned_count > 0 {
            env.storage()
                .instance()
                .set(&DataKey::PendingOps, &new_pending);
            env.storage()
                .instance()
                .set(&DataKey::FinalizedOps, &new_finalized);
            env.events().publish(
                (Symbol::new(&env, router_common::EVENT_OPS_CLEANED),),
                cleaned_count,
            );
        }

        Ok(cleaned_count)
    }

    /// Get an operation by id.
    pub fn get_op(env: Env, op_id: Bytes) -> Option<Op> {
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        env.storage().instance().get(&DataKey::Op(op_id))
    }

    /// Get the dependency operation IDs stored for `op_id`, if any.
    ///
    /// Returns an empty `Vec` when no dependencies were recorded (i.e. the
    /// operation was queued without deps, or the `op_id` does not exist).
    pub fn get_dependencies(env: Env, op_id: Bytes) -> Vec<Bytes> {
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        env.storage()
            .instance()
            .get(&DataKey::Deps(op_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get the human-readable status of an operation.
    ///
    /// # Returns
    /// * `Cancelled` — if the operation was cancelled.
    /// * `Executed`  — if the operation was executed.
    /// * `Expired`   — if `now > eta + grace_period_seconds` (and not executed/cancelled).
    /// * `Blocked`   — if a direct dependency was cancelled (and not expired/executed/cancelled).
    /// * `Ready`     — if `now >= eta` and still within the grace period.
    /// * `Queued`    — if `now < eta`.
    ///
    /// Returns `None` if no operation with `op_id` exists.
    pub fn get_operation_status(env: Env, op_id: Bytes) -> Option<OperationStatus> {
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        let op: Op = env.storage().instance().get(&DataKey::Op(op_id.clone()))?;
        let now = env.ledger().timestamp();
        let status = if op.cancelled {
            OperationStatus::Cancelled
        } else if op.executed {
            OperationStatus::Executed
        } else if op
            .eta
            .checked_add(op.grace_period_seconds)
            // Overflow is treated as expired, consistent with cleanup_expired
            // and the other status-reporting functions.
            .is_none_or(|expiry| now > expiry)
        {
            OperationStatus::Expired
        } else if Self::has_blocked_dependency(&env, &op_id) {
            OperationStatus::Blocked
        } else if now >= op.eta {
            OperationStatus::Ready
        } else {
            OperationStatus::Queued
        };
        Some(status)
    }

    /// Get all pending operations efficiently using the pending ops index.
    ///
    /// Loads only the operation IDs that have been tracked in the pending ops
    /// index and filters to return only those that are genuinely pending
    /// (not executed, not cancelled, and within their grace period).
    /// This is O(pending) instead of O(total storage scan).
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// A [`Vec<Op>`] of all pending operations.
    pub fn get_pending_operations(env: Env) -> Vec<Op> {
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        let pending: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::PendingOps)
            .unwrap_or_else(|| Vec::new(&env));
        let now = env.ledger().timestamp();
        let mut result = Vec::new(&env);
        for op_id in pending.iter() {
            if let Some(op) = env
                .storage()
                .instance()
                .get::<DataKey, Op>(&DataKey::Op(op_id))
            {
                // Only include ops that are genuinely pending (not expired).
                // checked_add: treat overflow as expired (op cannot be executed).
                let within_grace = op
                    .eta
                    .checked_add(op.grace_period_seconds)
                    .is_some_and(|expiry| now <= expiry);
                if !op.executed && !op.cancelled && within_grace {
                    result.push_back(op);
                }
            }
        }
        result
    }

    /// Get the count of operations by status.
    ///
    /// Iterates the pending ops index to compute counts efficiently without
    /// loading all operation data. Useful for dashboards and monitoring.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `status` - The [`OperationStatus`] to count.
    ///
    /// # Returns
    /// The count of operations matching the given status.
    pub fn get_operation_count_by_status(env: Env, status: OperationStatus) -> u32 {
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        let pending: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::PendingOps)
            .unwrap_or_else(|| Vec::new(&env));
        let now = env.ledger().timestamp();
        let mut count = 0u32;
        for op_id in pending.iter() {
            if let Some(op) = env
                .storage()
                .instance()
                .get::<DataKey, Op>(&DataKey::Op(op_id.clone()))
            {
                let matches = match status {
                    OperationStatus::Cancelled => op.cancelled,
                    OperationStatus::Executed => op.executed,
                    OperationStatus::Expired => {
                        let is_expired = op
                            .eta
                            .checked_add(op.grace_period_seconds)
                            .is_none_or(|expiry| now > expiry);
                        !op.executed && !op.cancelled && is_expired
                    }
                    OperationStatus::Ready => {
                        let within_grace = op
                            .eta
                            .checked_add(op.grace_period_seconds)
                            .is_some_and(|expiry| now <= expiry);
                        !op.executed
                            && !op.cancelled
                            && now >= op.eta
                            && within_grace
                            && !Self::has_blocked_dependency(&env, &op_id)
                    }
                    OperationStatus::Queued => {
                        !op.executed
                            && !op.cancelled
                            && now < op.eta
                            && !Self::has_blocked_dependency(&env, &op_id)
                    }
                    OperationStatus::Blocked => {
                        let within_grace = op
                            .eta
                            .checked_add(op.grace_period_seconds)
                            .is_some_and(|expiry| now <= expiry);
                        !op.executed
                            && !op.cancelled
                            && within_grace
                            && Self::has_blocked_dependency(&env, &op_id)
                    }
                };
                if matches {
                    count += 1;
                }
            }
        }
        count
    }

    /// Get all operations matching a specific status.
    pub fn get_operations_by_status(env: Env, status: OperationStatus) -> Vec<(Bytes, Op)> {
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        let pending: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::PendingOps)
            .unwrap_or_else(|| Vec::new(&env));

        let now = env.ledger().timestamp();
        let mut result = Vec::new(&env);

        for op_id in pending.iter() {
            if let Some(op) = env
                .storage()
                .instance()
                .get::<DataKey, Op>(&DataKey::Op(op_id.clone()))
            {
                let matches = match status {
                    OperationStatus::Cancelled => op.cancelled,
                    OperationStatus::Executed => op.executed,
                    OperationStatus::Expired => {
                        let is_expired = op
                            .eta
                            .checked_add(op.grace_period_seconds)
                            .is_none_or(|expiry| now > expiry);
                        !op.executed && !op.cancelled && is_expired
                    }
                    OperationStatus::Ready => {
                        let within_grace = op
                            .eta
                            .checked_add(op.grace_period_seconds)
                            .is_some_and(|expiry| now <= expiry);
                        !op.executed
                            && !op.cancelled
                            && now >= op.eta
                            && within_grace
                            && !Self::has_blocked_dependency(&env, &op_id)
                    }
                    OperationStatus::Queued => {
                        !op.executed
                            && !op.cancelled
                            && now < op.eta
                            && !Self::has_blocked_dependency(&env, &op_id)
                    }
                    OperationStatus::Blocked => {
                        let within_grace = op
                            .eta
                            .checked_add(op.grace_period_seconds)
                            .is_some_and(|expiry| now <= expiry);
                        !op.executed
                            && !op.cancelled
                            && within_grace
                            && Self::has_blocked_dependency(&env, &op_id)
                    }
                };

                if matches {
                    result.push_back((op_id.clone(), op));
                }
            }
        }

        result
    }

    /// Get the maximum allowed number of pending operations.
    ///
    /// A value of `0` means the pending-operations cap is disabled (unlimited).
    pub fn get_max_pending_ops(env: Env) -> u32 {
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        env.storage()
            .instance()
            .get(&DataKey::MaxPendingOps)
            .unwrap_or(0)
    }

    /// Get the minimum delay.
    ///
    /// # Errors
    /// Returns `TimelockError::NotInitialized` if the contract has not been initialized.
    pub fn min_delay(env: Env) -> Result<u64, TimelockError> {
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        env.storage()
            .instance()
            .get(&DataKey::MinDelay)
            .ok_or(TimelockError::NotInitialized)
    }

    /// Update the minimum delay (seconds) required for newly queued operations.
    ///
    /// Only the admin may call this. The new value only applies to operations
    /// queued after this call — already-queued operations keep the `eta` that
    /// was computed from the delay in effect when they were queued, so this
    /// cannot be used to accelerate or stall operations already in the queue.
    ///
    /// Emits `min_delay_updated` with `(old_min_delay, new_min_delay)`.
    pub fn set_min_delay(
        env: Env,
        caller: Address,
        new_min_delay: u64,
    ) -> Result<(), TimelockError> {
        caller.require_auth();
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, TimelockError)?;

        if new_min_delay == 0 {
            return Err(TimelockError::DelayTooShort);
        }

        // Issue #1207: `require_admin_simple!` above already proved
        // `DataKey::Admin` exists, and `initialize` always sets `MinDelay`
        // alongside `Admin` in the same call with nothing in this contract
        // ever removing either key — so `MinDelay` is guaranteed present
        // here. The `NotInitialized` fallback this used to have was dead
        // code; `.expect` documents the invariant instead of silently
        // re-checking something already proven.
        let old_min_delay: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MinDelay)
            .expect("MinDelay is always set alongside Admin in initialize");

        env.storage()
            .instance()
            .set(&DataKey::MinDelay, &new_min_delay);

        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_MIN_DELAY_UPDATED),),
            (old_min_delay, new_min_delay),
        );

        Ok(())
    }

    /// Get the admin.
    ///
    /// # Errors
    /// Returns `TimelockError::NotInitialized` if the contract has not been initialized.
    pub fn admin(env: Env) -> Result<Address, TimelockError> {
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(TimelockError::NotInitialized)
    }

    /// Transfer admin to a new address.
    pub fn transfer_admin(
        env: Env,
        current: Address,
        new_admin: Address,
    ) -> Result<(), TimelockError> {
        current.require_auth();
        router_common::extend_instance_ttl(
            &env,
            Self::INSTANCE_TTL_THRESHOLD,
            Self::INSTANCE_TTL_EXTEND_TO,
        );
        router_common::require_admin_simple!(&env, &current, &DataKey::Admin, TimelockError)?;

        env.storage().instance().set(&DataKey::Admin, &new_admin);

        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_ADMIN_TRANSFERRED),),
            (current, new_admin),
        );

        Ok(())
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn require_op_pending(op: &Op) -> Result<(), TimelockError> {
        if op.cancelled {
            return Err(TimelockError::Cancelled);
        }
        if op.executed {
            return Err(TimelockError::AlreadyExecuted);
        }
        Ok(())
    }

    /// Add an operation ID to the pending ops index.
    ///
    /// Callers must ensure `op_id` is not already tracked — `queue` guarantees
    /// this via its `AlreadyQueued` check before calling this helper.
    fn add_to_pending_ops(env: &Env, op_id: &Bytes) {
        let mut pending: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::PendingOps)
            .unwrap_or_else(|| Vec::new(env));
        pending.push_back(op_id.clone());
        env.storage().instance().set(&DataKey::PendingOps, &pending);
    }

    /// Check dependency chain depth to prevent infinite recursion.
    /// Loads each dependency by ID and checks whether it itself exists
    /// as an operation, incrementing depth at each level.
    fn check_dependency_depth(env: &Env, dep_id: Bytes, depth: u32) -> Result<(), TimelockError> {
        if depth > Self::MAX_DEPENDENCY_DEPTH {
            return Err(TimelockError::DependencyTooDeep);
        }
        let children: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::Deps(dep_id))
            .unwrap_or_else(|| Vec::new(env));
        for child_id in children.iter() {
            Self::check_dependency_depth(env, child_id, depth + 1)?;
        }
        Ok(())
    }

    /// Fetch the list of dependency IDs for `op_id`, defaulting to an empty
    /// `Vec` when no entry exists.  This is the single place that knows the
    /// storage key and the missing-value default, so both
    /// `require_dependencies_executed` and `has_blocked_dependency` stay in
    /// sync automatically.
    fn load_deps(env: &Env, op_id: &Bytes) -> Vec<Bytes> {
        env.storage()
            .instance()
            .get(&DataKey::Deps(op_id.clone()))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Require that every dependency recorded for `op_id` (via `DataKey::Deps`)
    /// has itself been executed. A dependency that doesn't exist as an `Op`
    /// (or exists but hasn't executed yet) blocks execution.
    fn require_dependencies_executed(env: &Env, op_id: &Bytes) -> Result<(), TimelockError> {
        for dep_id in Self::load_deps(env, op_id).iter() {
            let dep_executed = env
                .storage()
                .instance()
                .get::<DataKey, Op>(&DataKey::Op(dep_id))
                .map(|dep_op| dep_op.executed)
                .unwrap_or(false);
            if !dep_executed {
                return Err(TimelockError::DependencyNotExecuted);
            }
        }
        Ok(())
    }

    /// Returns `true` if any dependency of `op_id`, direct or transitive, can
    /// never be executed — it was cancelled, it expired without executing,
    /// its storage no longer exists, or it is itself blocked by one of *its*
    /// dependencies — meaning this operation can never execute either
    /// (`require_dependencies_executed` will always fail). Recursion is
    /// bounded by `MAX_DEPENDENCY_DEPTH`, mirroring `check_dependency_depth`.
    fn has_blocked_dependency(env: &Env, op_id: &Bytes) -> bool {
        Self::has_blocked_dependency_at_depth(env, op_id, 0)
    }

    fn has_blocked_dependency_at_depth(env: &Env, op_id: &Bytes, depth: u32) -> bool {
        if depth > Self::MAX_DEPENDENCY_DEPTH {
            // Depth is already bounded on the way in by `check_dependency_depth`
            // at queue time, so this is a defensive backstop, not the primary
            // guard — treat an implausibly deep chain as blocked rather than
            // recursing further.
            return true;
        }
        let now = env.ledger().timestamp();
        for dep_id in Self::load_deps(env, op_id).iter() {
            match env
                .storage()
                .instance()
                .get::<DataKey, Op>(&DataKey::Op(dep_id.clone()))
            {
                Some(dep_op) => {
                    if dep_op.cancelled
                        || (!dep_op.executed && Self::is_expired(&dep_op, now))
                        || (!dep_op.executed
                            && Self::has_blocked_dependency_at_depth(env, &dep_id, depth + 1))
                    {
                        return true;
                    }
                }
                None => return true,
            }
        }
        false
    }

    /// Returns `true` if `now` is past `eta + grace_period_seconds`.
    /// Overflow is treated as expired, consistent with the status queries.
    fn is_expired(op: &Op, now: u64) -> bool {
        op.eta
            .checked_add(op.grace_period_seconds)
            .is_none_or(|expiry| now > expiry)
    }

    /// Add an executed or cancelled operation ID to the finalized ops index so
    /// `cleanup_expired` can later reclaim its storage.
    fn add_to_finalized_ops(env: &Env, op_id: &Bytes) {
        let mut finalized: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::FinalizedOps)
            .unwrap_or_else(|| Vec::new(env));
        finalized.push_back(op_id.clone());
        env.storage()
            .instance()
            .set(&DataKey::FinalizedOps, &finalized);
    }

    /// Delete the `Op` and `Deps` storage entries for `op_id`.
    fn remove_op_storage(env: &Env, op_id: &Bytes) {
        env.storage().instance().remove(&DataKey::Op(op_id.clone()));
        env.storage()
            .instance()
            .remove(&DataKey::Deps(op_id.clone()));
    }

    /// Remove an operation ID from the pending ops index.
    fn remove_from_pending_ops(env: &Env, op_id: &Bytes) {
        let pending: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::PendingOps)
            .unwrap_or_else(|| Vec::new(env));

        let mut new_pending = Vec::new(env);
        for id in pending.iter() {
            if id != *op_id {
                new_pending.push_back(id);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::PendingOps, &new_pending);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        Bytes, Env, IntoVal, String, Symbol,
    };

    /// Default grace period used in most tests: 24 hours.
    const GRACE: u64 = 86_400;

    fn setup() -> (Env, Address, RouterTimelockClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &3600, &1000);
        (env, admin, client)
    }

    // ── queue ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_queue_returns_op_id() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        assert!(!op_id.is_empty());
    }

    #[test]
    fn test_queue_emits_op_queued_event() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);

        let events = env.events().all();
        let last = events.last().unwrap();

        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, router_common::EVENT_OP_QUEUED));

        let (emitted_id, emitted_target, emitted_eta, emitted_grace): (Bytes, Address, u64, u64) =
            last.2.into_val(&env);
        assert_eq!(emitted_id, op_id);
        assert_eq!(emitted_target, target);
        assert!(emitted_eta > 0);
        assert_eq!(emitted_grace, GRACE);
    }

    #[test]
    fn test_queue_stores_op() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        let op = client.get_op(&op_id).unwrap();

        assert_eq!(op.target, target);
        assert_eq!(op.grace_period_seconds, GRACE);
        assert!(!op.executed);
        assert!(!op.cancelled);
    }

    #[test]
    fn test_get_op_nonexistent_returns_none() {
        let (env, _admin, client) = setup();
        let fake_id = Bytes::from_array(&env, &[0u8; 32]);
        assert_eq!(client.get_op(&fake_id), None);
    }

    #[test]
    fn test_queue_stores_grace_period() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "check grace stored");
        let deps: Vec<Bytes> = Vec::new(&env);
        let custom_grace: u64 = 7200;

        let op_id = client.queue(&admin, &desc, &target, &3600, &custom_grace, &deps);
        let op = client.get_op(&op_id).unwrap();

        assert_eq!(op.grace_period_seconds, custom_grace);
    }

    // ── Issue #822: queue() op_id collision guard ────────────────────────────

    #[test]
    fn test_queue_rejects_op_id_collision_after_execution() {
        // op_id = sha256(description_xdr + target_xdr + eta_be_bytes), and
        // queue() always computes eta as `current_timestamp + delay` with
        // delay required to be > 0 (min_delay). So a *later* queue() call can
        // never legitimately reproduce an eta that a call at or before the
        // current timestamp already claimed: executing an op requires the
        // clock to have reached its eta, but any new delay computed from
        // that same (or later) clock can only push eta forward, never land
        // back on it. A collision with an already-*executed* op therefore
        // cannot arise from two ordinary queue() calls — so this test
        // arranges the precondition directly (an executed Op already
        // occupying the id a real queue() call is about to compute) rather
        // than trying to reach it through normal timestamp/delay mechanics.
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps: Vec<Bytes> = Vec::new(&env);
        let delay: u64 = 3600;

        let eta = env.ledger().timestamp() + delay;
        let mut preimage = Bytes::new(&env);
        preimage.append(&desc.clone().to_xdr(&env));
        preimage.append(&target.clone().to_xdr(&env));
        preimage.append(&Bytes::from_array(&env, &eta.to_be_bytes()));
        let op_id: Bytes = env.crypto().sha256(&preimage).into();

        env.as_contract(&client.address, || {
            let op = Op {
                proposer: admin.clone(),
                description: desc.clone(),
                target: target.clone(),
                eta,
                grace_period_seconds: GRACE,
                executed: true,
                cancelled: false,
            };
            env.storage()
                .instance()
                .set(&DataKey::Op(op_id.clone()), &op);
        });

        // A fresh queue() call with the same description/target/delay, made
        // right now, computes this exact op_id — and must be rejected rather
        // than silently resetting the already-executed op back to pending.
        let result = client.try_queue(&admin, &desc, &target, &delay, &GRACE, &deps);
        assert_eq!(result, Err(Ok(TimelockError::AlreadyQueued)));

        let op_after = client.get_op(&op_id).unwrap();
        assert!(op_after.executed);
    }

    // ── execute ───────────────────────────────────────────────────────────────

    #[test]
    fn test_execute_before_eta_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        let result = client.try_execute(&admin, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::NotReady)));
    }

    #[test]
    fn test_execute_after_eta_succeeds() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);

        let op = client.get_op(&op_id).unwrap();
        assert!(op.executed);
    }

    // ── Issue #821: execute() enforces dependency completion ─────────────────

    #[test]
    fn test_execute_child_before_parent_executed_fails() {
        let (env, admin, client) = setup();
        let parent_target = Address::generate(&env);
        let child_target = Address::generate(&env);
        let parent_desc = String::from_str(&env, "register adapter");
        let child_desc = String::from_str(&env, "upgrade adapter");
        let no_deps: Vec<Bytes> = Vec::new(&env);

        let parent_id = client.queue(
            &admin,
            &parent_desc,
            &parent_target,
            &3600,
            &GRACE,
            &no_deps,
        );

        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(&admin, &child_desc, &child_target, &3600, &GRACE, &deps);

        env.ledger().with_mut(|l| l.timestamp += 3601);

        // Child cannot execute before its dependency (parent) has executed.
        let result = client.try_execute(&admin, &child_id);
        assert_eq!(result, Err(Ok(TimelockError::DependencyNotExecuted)));

        // Once the parent executes, the child can execute too.
        client.execute(&admin, &parent_id);
        client.execute(&admin, &child_id);

        let child_op = client.get_op(&child_id).unwrap();
        assert!(child_op.executed);
    }

    #[test]
    fn test_execute_after_grace_period_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600; // 1-hour grace window

        let op_id = client.queue(&admin, &desc, &target, &3600, &grace, &deps);
        // Jump past eta + grace_period_seconds
        env.ledger().with_mut(|l| l.timestamp += 3600 + grace + 1);
        let result = client.try_execute(&admin, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::Expired)));
    }

    #[test]
    fn test_execute_at_grace_period_boundary_succeeds() {
        // Execution exactly at eta + grace_period_seconds is still valid (inclusive boundary).
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "boundary test");
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        let op_id = client.queue(&admin, &desc, &target, &3600, &grace, &deps);
        // Jump to exactly eta + grace_period_seconds
        env.ledger().with_mut(|l| l.timestamp += 3600 + grace);
        client.execute(&admin, &op_id);

        let op = client.get_op(&op_id).unwrap();
        assert!(op.executed);
    }

    #[test]
    fn test_execute_cancelled_op_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        client.cancel(&admin, &op_id);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        let result = client.try_execute(&admin, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::Cancelled)));
    }

    #[test]
    fn test_execute_twice_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);
        let result = client.try_execute(&admin, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::AlreadyExecuted)));
    }

    #[test]
    fn test_execute_emits_op_executed_event() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);

        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, router_common::EVENT_OP_EXECUTED));
    }

    #[test]
    fn test_execute_nonexistent_op_fails() {
        let (env, admin, client) = setup();
        let fake_id = Bytes::from_array(&env, &[0u8; 32]);
        let result = client.try_execute(&admin, &fake_id);
        assert_eq!(result, Err(Ok(TimelockError::NotFound)));
    }

    #[test]
    fn test_execute_unauthorized_fails() {
        let (env, admin, client) = setup();
        let attacker = Address::generate(&env);
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);

        let result = client.try_execute(&attacker, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::Unauthorized)));
    }

    // ── cancel ────────────────────────────────────────────────────────────────

    #[test]
    fn test_cancel_op() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        client.cancel(&admin, &op_id);

        let op = client.get_op(&op_id).unwrap();
        assert!(op.cancelled);
    }

    #[test]
    fn test_cancel_emits_op_cancelled_event() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        client.cancel(&admin, &op_id);

        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, router_common::EVENT_OP_CANCELLED));
    }

    #[test]
    fn test_cancel_nonexistent_op_fails() {
        let (env, admin, client) = setup();
        let fake_id = Bytes::from_array(&env, &[0u8; 32]);
        assert_eq!(
            client.try_cancel(&admin, &fake_id),
            Err(Ok(TimelockError::NotFound))
        );
    }

    #[test]
    fn test_cancel_unauthorized_fails() {
        let (env, admin, client) = setup();
        let attacker = Address::generate(&env);
        let target = Address::generate(&env);
        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "d"),
            &target,
            &3600,
            &GRACE,
            &Vec::new(&env),
        );
        assert_eq!(
            client.try_cancel(&attacker, &op_id),
            Err(Ok(TimelockError::Unauthorized))
        );
    }

    #[test]
    fn test_cancel_already_cancelled_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "d"),
            &target,
            &3600,
            &GRACE,
            &Vec::new(&env),
        );
        client.cancel(&admin, &op_id);
        assert_eq!(
            client.try_cancel(&admin, &op_id),
            Err(Ok(TimelockError::Cancelled))
        );
    }

    #[test]
    fn test_cancel_already_executed_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "d"),
            &target,
            &3600,
            &GRACE,
            &Vec::new(&env),
        );
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);
        assert_eq!(
            client.try_cancel(&admin, &op_id),
            Err(Ok(TimelockError::AlreadyExecuted))
        );
    }

    // ── validation ────────────────────────────────────────────────────────────

    #[test]
    fn test_initialize_rejects_zero_min_delay() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        let result = client.try_initialize(&admin, &0, &1000);
        assert_eq!(result, Err(Ok(TimelockError::DelayTooShort)));
    }

    #[test]
    fn test_set_min_delay_rejects_zero() {
        let (_env, admin, client) = setup();
        let result = client.try_set_min_delay(&admin, &0);
        assert_eq!(result, Err(Ok(TimelockError::DelayTooShort)));
    }

    #[test]
    fn test_delay_too_short_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        // min_delay is 3600, passing 100 should fail
        let result = client.try_queue(&admin, &desc, &target, &100, &GRACE, &deps);
        assert_eq!(result, Err(Ok(TimelockError::DelayTooShort)));
    }

    #[test]
    fn test_delay_overflow_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &1, &1000);

        env.ledger().with_mut(|l| l.timestamp = u64::MAX);

        let target = Address::generate(&env);
        let desc = String::from_str(&env, "overflow delay");
        let deps = Vec::new(&env);
        let result = client.try_queue(&admin, &desc, &target, &1, &GRACE, &deps);
        assert_eq!(result, Err(Ok(TimelockError::DelayTooLong)));
    }

    #[test]
    fn test_unauthorized_queue_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let result = client.try_queue(&attacker, &desc, &target, &3600, &GRACE, &deps);
        assert_eq!(result, Err(Ok(TimelockError::Unauthorized)));
    }

    // ── get_operation_status ──────────────────────────────────────────────────

    #[test]
    fn test_get_operation_status_queued() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        assert_eq!(
            client.get_operation_status(&op_id),
            Some(OperationStatus::Queued)
        );
    }

    #[test]
    fn test_get_operation_status_ready() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        // Past ETA but still within grace period
        env.ledger().with_mut(|l| l.timestamp += 3601);
        assert_eq!(
            client.get_operation_status(&op_id),
            Some(OperationStatus::Ready)
        );
    }

    #[test]
    fn test_get_operation_status_executed() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);
        assert_eq!(
            client.get_operation_status(&op_id),
            Some(OperationStatus::Executed)
        );
    }

    #[test]
    fn test_get_operation_status_cancelled() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        client.cancel(&admin, &op_id);
        assert_eq!(
            client.get_operation_status(&op_id),
            Some(OperationStatus::Cancelled)
        );
    }

    #[test]
    fn test_get_operation_status_expired() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        let op_id = client.queue(&admin, &desc, &target, &3600, &grace, &deps);
        // Jump past eta + grace_period_seconds
        env.ledger().with_mut(|l| l.timestamp += 3600 + grace + 1);
        assert_eq!(
            client.get_operation_status(&op_id),
            Some(OperationStatus::Expired)
        );
    }

    #[test]
    fn test_get_operation_status_nonexistent_returns_none() {
        let (env, _admin, client) = setup();
        let fake_id = Bytes::from_array(&env, &[0u8; 32]);
        assert_eq!(client.get_operation_status(&fake_id), None);
    }

    // ── update_description ────────────────────────────────────────────────────

    #[test]
    fn test_update_description_succeeds() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "initial desc"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        let new_desc = String::from_str(&env, "corrected desc");
        client.update_description(&admin, &op_id, &new_desc);

        let op = client.get_op(&op_id).unwrap();
        assert_eq!(op.description, new_desc);
    }

    #[test]
    fn test_update_description_emits_event() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "initial desc"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        let new_desc = String::from_str(&env, "corrected desc");
        client.update_description(&admin, &op_id, &new_desc);

        let events = env.events().all();
        let last = events.last().unwrap();

        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(
            topic,
            Symbol::new(&env, router_common::EVENT_OP_DESCRIPTION_UPDATED)
        );

        let (emitted_id, emitted_desc): (Bytes, String) = last.2.into_val(&env);
        assert_eq!(emitted_id, op_id);
        assert_eq!(emitted_desc, new_desc);
    }

    #[test]
    fn test_update_description_on_executed_op_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "initial desc"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);

        let result =
            client.try_update_description(&admin, &op_id, &String::from_str(&env, "too late"));
        assert_eq!(result, Err(Ok(TimelockError::AlreadyExecuted)));
    }

    #[test]
    fn test_update_description_on_cancelled_op_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "initial desc"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        client.cancel(&admin, &op_id);

        let result =
            client.try_update_description(&admin, &op_id, &String::from_str(&env, "too late"));
        assert_eq!(result, Err(Ok(TimelockError::Cancelled)));
    }

    #[test]
    fn test_update_description_nonexistent_op_fails() {
        let (env, admin, client) = setup();
        let fake_id = Bytes::from_array(&env, &[0u8; 32]);

        let result =
            client.try_update_description(&admin, &fake_id, &String::from_str(&env, "ghost op"));
        assert_eq!(result, Err(Ok(TimelockError::NotFound)));
    }

    #[test]
    fn test_update_description_unauthorized_fails() {
        let (env, admin, client) = setup();
        let attacker = Address::generate(&env);
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);

        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "initial desc"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        let result =
            client.try_update_description(&attacker, &op_id, &String::from_str(&env, "hacked"));
        assert_eq!(result, Err(Ok(TimelockError::Unauthorized)));
    }

    #[test]
    fn test_update_description_ready_op_succeeds() {
        // An op that is past its ETA but not yet executed is still pending — update should work.
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "initial desc"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        env.ledger().with_mut(|l| l.timestamp += 3601);

        let new_desc = String::from_str(&env, "clarified before execution");
        client.update_description(&admin, &op_id, &new_desc);

        let op = client.get_op(&op_id).unwrap();
        assert_eq!(op.description, new_desc);
    }

    // ── transfer_admin ────────────────────────────────────────────────────────

    #[test]
    fn test_transfer_admin() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);
        assert_eq!(client.admin(), new_admin);
    }

    #[test]
    fn test_transfer_admin_emits_event() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);
        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, "admin_transferred"));
        let (old, new): (Address, Address) = last.2.into_val(&env);
        assert_eq!(old, admin);
        assert_eq!(new, new_admin);
    }

    #[test]
    fn test_transfer_admin_old_admin_locked_out() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);
        // old admin can no longer call privileged functions
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "locked out test");
        let deps = Vec::new(&env);
        assert_eq!(
            client.try_queue(&admin, &desc, &target, &3600, &GRACE, &deps),
            Err(Ok(TimelockError::Unauthorized))
        );
    }

    // ── set_min_delay ─────────────────────────────────────────────────────────

    #[test]
    fn test_set_min_delay_updates_value() {
        let (_env, admin, client) = setup();
        client.set_min_delay(&admin, &7200);
        assert_eq!(client.min_delay(), 7200);
    }

    #[test]
    fn test_set_min_delay_emits_event() {
        let (env, admin, client) = setup();
        client.set_min_delay(&admin, &7200);

        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(
            topic,
            Symbol::new(&env, router_common::EVENT_MIN_DELAY_UPDATED)
        );

        let (old, new): (u64, u64) = last.2.into_val(&env);
        assert_eq!(old, 3600);
        assert_eq!(new, 7200);
    }

    #[test]
    fn test_set_min_delay_unauthorized_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let result = client.try_set_min_delay(&attacker, &7200);
        assert_eq!(result, Err(Ok(TimelockError::Unauthorized)));
    }

    #[test]
    fn test_set_min_delay_does_not_affect_already_queued_ops() {
        // Operations queued before a min_delay change keep the eta computed
        // from the delay that was in effect at queue time.
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        let op_before = client.get_op(&op_id).unwrap();

        // Raise min_delay well above the original queued delay.
        client.set_min_delay(&admin, &100_000);

        let op_after = client.get_op(&op_id).unwrap();
        assert_eq!(op_before.eta, op_after.eta);

        // The op still becomes executable at its original eta, unaffected
        // by the new (higher) min_delay.
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);
        assert!(client.get_op(&op_id).unwrap().executed);
    }

    #[test]
    fn test_set_min_delay_applies_to_newly_queued_ops() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        client.set_min_delay(&admin, &7200);

        // A delay below the new min_delay (but above the old one) must fail.
        let result = client.try_queue(&admin, &desc, &target, &3600, &GRACE, &deps);
        assert_eq!(result, Err(Ok(TimelockError::DelayTooShort)));
    }

    // ── Issue #586: pending ops index and count_by_status ─────────────────────

    #[test]
    fn test_get_pending_operations_returns_only_pending() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        // Initially empty
        assert!(client.get_pending_operations().is_empty());

        // Queue two ops
        let op1 = client.queue(
            &admin,
            &String::from_str(&env, "op1"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        let op2 = client.queue(
            &admin,
            &String::from_str(&env, "op2"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        let pending = client.get_pending_operations();
        assert_eq!(pending.len(), 2);

        // Cancel op1 — should drop to 1 pending
        client.cancel(&admin, &op1);
        assert_eq!(client.get_pending_operations().len(), 1);

        // Execute op2 — should be empty
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op2);
        assert_eq!(client.get_pending_operations().len(), 0);
    }

    #[test]
    fn test_get_operation_count_by_status_counts_correctly() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        // Queue two ops
        client.queue(
            &admin,
            &String::from_str(&env, "op1"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        client.queue(
            &admin,
            &String::from_str(&env, "op2"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        // Both should be Queued
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Queued),
            2
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Ready),
            0
        );

        // Advance past ETA — both become Ready
        env.ledger().with_mut(|l| l.timestamp += 3601);
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Queued),
            0
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Ready),
            2
        );
    }

    #[test]
    fn test_get_operation_count_by_status_expired() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        client.queue(
            &admin,
            &String::from_str(&env, "expires"),
            &target,
            &3600,
            &grace,
            &deps,
        );

        // Jump past grace period
        env.ledger().with_mut(|l| l.timestamp += 3600 + grace + 1);
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Expired),
            1
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Ready),
            0
        );
    }

    #[test]
    fn test_get_operations_by_status_returns_matching_ops() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        let op1 = client.queue(
            &admin,
            &String::from_str(&env, "op1"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        let op2 = client.queue(
            &admin,
            &String::from_str(&env, "op2"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        let queued = client.get_operations_by_status(&OperationStatus::Queued);
        assert_eq!(queued.len(), 2);
        assert_eq!(queued.get(0).unwrap().0, op1);
        assert_eq!(queued.get(1).unwrap().0, op2);

        let ready = client.get_operations_by_status(&OperationStatus::Ready);
        assert_eq!(ready.len(), 0);

        // Advance past ETA — both become Ready
        env.ledger().with_mut(|l| l.timestamp += 3601);

        let queued = client.get_operations_by_status(&OperationStatus::Queued);
        assert_eq!(queued.len(), 0);

        let ready = client.get_operations_by_status(&OperationStatus::Ready);
        assert_eq!(ready.len(), 2);
        assert_eq!(ready.get(0).unwrap().0, op1);
        assert_eq!(ready.get(1).unwrap().0, op2);
    }

    #[test]
    fn test_get_operations_by_status_expired() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "expires"),
            &target,
            &3600,
            &grace,
            &deps,
        );

        // Jump past grace period
        env.ledger().with_mut(|l| l.timestamp += 3600 + grace + 1);
        let expired = client.get_operations_by_status(&OperationStatus::Expired);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired.get(0).unwrap().0, op_id);

        let ready = client.get_operations_by_status(&OperationStatus::Ready);
        assert_eq!(ready.len(), 0);
    }

    #[test]
    fn test_pending_ops_index_excludes_expired_from_pending() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        client.queue(
            &admin,
            &String::from_str(&env, "expires"),
            &target,
            &3600,
            &grace,
            &deps,
        );

        // Before grace period expires, it's pending
        env.ledger().with_mut(|l| l.timestamp += 3601);
        assert_eq!(client.get_pending_operations().len(), 1);

        // After grace period, it's no longer pending
        env.ledger().with_mut(|l| l.timestamp += grace);
        assert_eq!(client.get_pending_operations().len(), 0);
    }

    #[test]
    fn test_get_pending_operations_is_efficient_with_many_cancelled() {
        // Queue many ops, cancel most — pending ops index should stay small
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);
        let grace: u64 = 3600;

        // Queue 5 ops with unique descriptions to get unique op_ids
        let mut op_ids = Vec::new(&env);
        for i in 0..5u64 {
            let desc_str = std::format!("op_{}", i);
            let desc = String::from_str(&env, &desc_str);
            let op_id = client.queue(&admin, &desc, &target, &3600, &grace, &deps);
            let id: Bytes = op_id;
            op_ids.push_back(id);
        }

        // Cancel 4 of them
        for i in 0..4u32 {
            let id = op_ids.get(i).unwrap();
            client.cancel(&admin, &id);
        }

        // Only 1 should remain pending
        assert_eq!(client.get_pending_operations().len(), 1);

        // get_operation_count_by_status should reflect the state.
        // Since cancelled ops are completely removed, they count as 0.
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Cancelled),
            0
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Queued),
            1
        );
    }

    // ── QueueFull limit ───────────────────────────────────────────────────────

    #[test]
    fn test_get_max_pending_ops_returns_initialized_value() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &3600, &50);
        assert_eq!(client.get_max_pending_ops(), 50);
    }

    #[test]
    fn test_queue_unlimited_when_max_pending_ops_is_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        // 0 disables the pending-operations cap
        client.initialize(&admin, &3600, &0);
        assert_eq!(client.get_max_pending_ops(), 0);

        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);

        for i in 0..25 {
            let desc = std::format!("op_{}", i);
            let result = client.try_queue(
                &admin,
                &String::from_str(&env, &desc),
                &target,
                &3600,
                &GRACE,
                &deps,
            );
            assert!(result.is_ok(), "queue #{} failed: {:?}", i, result);
        }
        assert_eq!(client.get_pending_operations().len(), 25);
    }

    #[test]
    fn test_queue_fails_when_pending_limit_reached() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        // Set max to 2
        client.initialize(&admin, &3600, &2);

        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);

        client.queue(
            &admin,
            &String::from_str(&env, "op1"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        client.queue(
            &admin,
            &String::from_str(&env, "op2"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        // Third queue should fail with QueueFull
        let result = client.try_queue(
            &admin,
            &String::from_str(&env, "op3"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        assert_eq!(result, Err(Ok(TimelockError::QueueFull)));
    }

    #[test]
    fn test_queue_succeeds_after_cancel_frees_slot() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        // Set max to 2
        client.initialize(&admin, &3600, &2);

        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);

        let op1 = client.queue(
            &admin,
            &String::from_str(&env, "op1"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        let _op2 = client.queue(
            &admin,
            &String::from_str(&env, "op2"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        // Cancel op1 to free a slot
        client.cancel(&admin, &op1);

        // Now queue should succeed
        let result = client.try_queue(
            &admin,
            &String::from_str(&env, "op3"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        assert!(result.is_ok());
    }

    // ── cleanup_expired ───────────────────────────────────────────────────────

    #[test]
    fn test_cleanup_expired_removes_expired_ops() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        // Queue two ops: one that will expire, one that won't
        let _op1 = client.queue(
            &admin,
            &String::from_str(&env, "expires"),
            &target,
            &3600,
            &grace,
            &deps,
        );
        let _op2 = client.queue(
            &admin,
            &String::from_str(&env, "stays"),
            &target,
            &7200,
            &GRACE,
            &deps,
        );

        // Jump past the grace period of op1
        let now = env.ledger().timestamp();
        env.ledger()
            .with_mut(|l| l.timestamp = now + 3600 + grace + 1);

        // Cleanup limit of 10 should remove the expired one
        let cleaned = client.cleanup_expired(&admin, &10);
        assert_eq!(cleaned, 1);

        // Verify op1 is no longer in pending ops
        assert_eq!(client.get_pending_operations().len(), 1);
        let pending = client.get_pending_operations();
        assert_eq!(pending.get(0).unwrap().target, target);
    }

    #[test]
    fn test_cleanup_expired_respects_limit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &3600, &100);

        let _target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        // Queue 5 ops with unique targets to get unique op_ids
        for _i in 0..5u32 {
            let target_i = Address::generate(&env);
            client.queue(
                &admin,
                &String::from_str(&env, "op"),
                &target_i,
                &3600,
                &grace,
                &deps,
            );
        }

        // Jump past grace period
        let now = env.ledger().timestamp();
        env.ledger()
            .with_mut(|l| l.timestamp = now + 3600 + grace + 1);

        // Cleanup with limit of 2
        let cleaned = client.cleanup_expired(&admin, &2);
        assert_eq!(cleaned, 2);

        // 3 should still remain in the index (not cleaned due to limit). They're
        // already expired, so get_pending_operations() (which excludes expired ops)
        // can't observe them; count by status against the raw index instead.
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Expired),
            3
        );

        // Cleanup remaining
        let cleaned2 = client.cleanup_expired(&admin, &10);
        assert_eq!(cleaned2, 3);
        assert_eq!(client.get_pending_operations().len(), 0);
    }

    #[test]
    fn test_cleanup_expired_removes_executed_and_cancelled() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        // Queue 3 ops
        let op1 = client.queue(
            &admin,
            &String::from_str(&env, "op1"),
            &target,
            &3600,
            &grace,
            &deps,
        );
        let op2 = client.queue(
            &admin,
            &String::from_str(&env, "op2"),
            &target,
            &3600,
            &grace,
            &deps,
        );
        let op3 = client.queue(
            &admin,
            &String::from_str(&env, "op3"),
            &target,
            &3600,
            &grace,
            &deps,
        );

        // Cancel op1
        client.cancel(&admin, &op1);

        // Execute op2 (advance time past ETA)
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op2);

        // op3 remains but will be expired
        let now = env.ledger().timestamp();
        env.ledger().with_mut(|l| l.timestamp = now + grace + 1);

        // Cancelled op1, executed op2 and expired op3 are all reclaimed.
        let cleaned = client.cleanup_expired(&admin, &10);
        assert_eq!(cleaned, 3);

        assert_eq!(client.get_op(&op1), None);
        assert_eq!(client.get_op(&op2), None);
        assert_eq!(client.get_op(&op3), None);
        assert_eq!(client.get_pending_operations().len(), 0);

        // Nothing left to reclaim.
        assert_eq!(client.cleanup_expired(&admin, &10), 0);
    }

    #[test]
    fn test_cleanup_expired_frees_deps_of_finalized_ops() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let no_deps: Vec<Bytes> = Vec::new(&env);

        let parent_id = client.queue(
            &admin,
            &String::from_str(&env, "parent"),
            &target,
            &3600,
            &GRACE,
            &no_deps,
        );
        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(
            &admin,
            &String::from_str(&env, "child"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &parent_id);
        client.execute(&admin, &child_id);

        assert_eq!(client.cleanup_expired(&admin, &10), 2);
        assert_eq!(client.get_op(&child_id), None);
        assert_eq!(client.get_dependencies(&child_id).len(), 0);
    }

    #[test]
    fn test_cleanup_expired_respects_limit_for_finalized_ops() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);

        for i in 0..3u32 {
            let desc = std::format!("op_{}", i);
            let op_id = client.queue(
                &admin,
                &String::from_str(&env, &desc),
                &target,
                &3600,
                &GRACE,
                &deps,
            );
            client.cancel(&admin, &op_id);
        }

        assert_eq!(client.cleanup_expired(&admin, &2), 2);
        assert_eq!(client.cleanup_expired(&admin, &10), 1);
        assert_eq!(client.cleanup_expired(&admin, &10), 0);
    }

    #[test]
    fn test_cleanup_expired_keeps_executed_dependency_of_live_op() {
        // Reclaiming an executed parent while a live child still depends on
        // it would make the child permanently unexecutable.
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let no_deps: Vec<Bytes> = Vec::new(&env);

        let parent_id = client.queue(
            &admin,
            &String::from_str(&env, "parent"),
            &target,
            &3600,
            &GRACE,
            &no_deps,
        );
        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(
            &admin,
            &String::from_str(&env, "child"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &parent_id);

        assert_eq!(client.cleanup_expired(&admin, &10), 0);
        assert!(client.get_op(&parent_id).unwrap().executed);
        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Ready)
        );

        client.execute(&admin, &child_id);

        // With the child finalized, both can now be reclaimed.
        assert_eq!(client.cleanup_expired(&admin, &10), 2);
        assert_eq!(client.get_op(&parent_id), None);
        assert_eq!(client.get_op(&child_id), None);
    }

    #[test]
    fn test_cleanup_expired_keeps_cancelled_dependency_of_live_op() {
        // Reclaiming a cancelled parent would hide the child's Blocked status.
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let no_deps: Vec<Bytes> = Vec::new(&env);

        let parent_id = client.queue(
            &admin,
            &String::from_str(&env, "parent"),
            &target,
            &3600,
            &GRACE,
            &no_deps,
        );
        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(
            &admin,
            &String::from_str(&env, "child"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        client.cancel(&admin, &parent_id);

        assert_eq!(client.cleanup_expired(&admin, &10), 0);
        assert!(client.get_op(&parent_id).unwrap().cancelled);
        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Blocked)
        );
    }

    #[test]
    fn test_cleanup_expired_keeps_expired_dependency_of_live_op() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let no_deps: Vec<Bytes> = Vec::new(&env);
        let short_grace: u64 = 3600;

        let parent_id = client.queue(
            &admin,
            &String::from_str(&env, "parent"),
            &target,
            &3600,
            &short_grace,
            &no_deps,
        );
        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(
            &admin,
            &String::from_str(&env, "child"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        // Parent expires; child is still within its own grace period.
        env.ledger()
            .with_mut(|l| l.timestamp += 3600 + short_grace + 1);

        assert_eq!(client.cleanup_expired(&admin, &10), 0);
        assert!(client.get_op(&parent_id).is_some());
        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Blocked)
        );

        // Once the child expires too, both are reclaimed.
        env.ledger().with_mut(|l| l.timestamp += GRACE);
        assert_eq!(client.cleanup_expired(&admin, &10), 2);
        assert_eq!(client.get_op(&parent_id), None);
        assert_eq!(client.get_op(&child_id), None);
    }

    // ── Blocked status ────────────────────────────────────────────────────────

    #[test]
    fn test_operation_status_blocked_when_dependency_cancelled() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let no_deps: Vec<Bytes> = Vec::new(&env);

        let parent_id = client.queue(
            &admin,
            &String::from_str(&env, "parent"),
            &target,
            &3600,
            &GRACE,
            &no_deps,
        );
        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(
            &admin,
            &String::from_str(&env, "child"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Queued)
        );

        client.cancel(&admin, &parent_id);

        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Blocked)
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Blocked),
            1
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Queued),
            0
        );
        let blocked = client.get_operations_by_status(&OperationStatus::Blocked);
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked.get(0).unwrap().0, child_id);
        assert_eq!(
            client
                .get_operations_by_status(&OperationStatus::Queued)
                .len(),
            0
        );

        // Still Blocked (not Ready) once the child's ETA has passed.
        env.ledger().with_mut(|l| l.timestamp += 3601);
        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Blocked)
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Ready),
            0
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Blocked),
            1
        );
    }

    #[test]
    fn test_operation_status_blocked_transitively_through_grandparent() {
        // A <- B <- C: only A (the root) is cancelled. B is never itself
        // cancelled, just permanently unexecutable — has_blocked_dependency
        // must walk the chain transitively so C is reported Blocked too,
        // not Ready/Queued (regression test for #1372).
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let no_deps: Vec<Bytes> = Vec::new(&env);

        let op_a = client.queue(
            &admin,
            &String::from_str(&env, "a"),
            &target,
            &3600,
            &GRACE,
            &no_deps,
        );

        let mut deps_b = Vec::new(&env);
        deps_b.push_back(op_a.clone());
        let op_b = client.queue(
            &admin,
            &String::from_str(&env, "b"),
            &target,
            &3600,
            &GRACE,
            &deps_b,
        );

        let mut deps_c = Vec::new(&env);
        deps_c.push_back(op_b.clone());
        let op_c = client.queue(
            &admin,
            &String::from_str(&env, "c"),
            &target,
            &3600,
            &GRACE,
            &deps_c,
        );

        client.cancel(&admin, &op_a);

        // B is directly blocked (its own dependency, A, was cancelled).
        assert_eq!(
            client.get_operation_status(&op_b),
            Some(OperationStatus::Blocked)
        );
        // C's only direct dependency is B, which was never cancelled itself —
        // this is exactly the case the transitive walk must catch.
        assert_eq!(
            client.get_operation_status(&op_c),
            Some(OperationStatus::Blocked)
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Blocked),
            2
        );
    }

    #[test]
    fn test_operation_status_blocked_when_dependency_expired() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let no_deps: Vec<Bytes> = Vec::new(&env);
        let short_grace: u64 = 3600;

        let parent_id = client.queue(
            &admin,
            &String::from_str(&env, "parent"),
            &target,
            &3600,
            &short_grace,
            &no_deps,
        );
        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(
            &admin,
            &String::from_str(&env, "child"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        // Parent is left to expire without being cancelled or executed.
        env.ledger()
            .with_mut(|l| l.timestamp += 3600 + short_grace + 1);

        assert_eq!(
            client.get_operation_status(&parent_id),
            Some(OperationStatus::Expired)
        );
        assert!(!client.get_op(&parent_id).unwrap().cancelled);
        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Blocked)
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Blocked),
            1
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Ready),
            0
        );
        let blocked = client.get_operations_by_status(&OperationStatus::Blocked);
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked.get(0).unwrap().0, child_id);

        let result = client.try_execute(&admin, &child_id);
        assert_eq!(result, Err(Ok(TimelockError::DependencyNotExecuted)));
    }

    #[test]
    fn test_operation_status_not_blocked_when_dependency_executed() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let no_deps: Vec<Bytes> = Vec::new(&env);
        let short_grace: u64 = 3600;

        let parent_id = client.queue(
            &admin,
            &String::from_str(&env, "parent"),
            &target,
            &3600,
            &short_grace,
            &no_deps,
        );
        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(
            &admin,
            &String::from_str(&env, "child"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &parent_id);

        // Parent's grace window passing after execution must not block the child.
        env.ledger().with_mut(|l| l.timestamp += short_grace);
        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Ready)
        );
        client.execute(&admin, &child_id);
    }

    #[test]
    fn test_cleanup_expired_emits_event() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        client.queue(
            &admin,
            &String::from_str(&env, "expires"),
            &target,
            &3600,
            &grace,
            &deps,
        );

        // Jump past grace period
        let now = env.ledger().timestamp();
        env.ledger()
            .with_mut(|l| l.timestamp = now + 3600 + grace + 1);

        client.cleanup_expired(&admin, &10);

        let events = env.events().all();
        let found = events.iter().any(|e| {
            let topic: Symbol = e.1.get(0).unwrap().into_val(&env);
            topic == Symbol::new(&env, "ops_cleaned")
        });
        assert!(found);
    }

    #[test]
    fn test_cleanup_expired_permissionless() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        client.initialize(&admin, &3600, &100);

        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        client.queue(
            &admin,
            &String::from_str(&env, "expires"),
            &target,
            &3600,
            &grace,
            &deps,
        );

        // Jump past grace period
        let now = env.ledger().timestamp();
        env.ledger()
            .with_mut(|l| l.timestamp = now + 3600 + grace + 1);

        // Non-admin can call cleanup
        let cleaned = client.try_cleanup_expired(&caller, &10);
        assert!(cleaned.unwrap().is_ok());
    }

    // ── dependency depth (#729) ───────────────────────────────────────────────

    #[test]
    fn test_queue_with_valid_dep_succeeds() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let dep_id = client.queue(
            &admin,
            &String::from_str(&env, "dep"),
            &target,
            &3600,
            &GRACE,
            &Vec::new(&env),
        );
        let mut deps = Vec::new(&env);
        deps.push_back(dep_id);
        let result = client.try_queue(
            &admin,
            &String::from_str(&env, "child"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_queue_circular_dependency_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "op_a");
        let delay: u64 = 3600;

        // Precompute the op_id the same way queue() does
        let eta = env.ledger().timestamp() + delay;
        let mut preimage = Bytes::new(&env);
        preimage.append(&desc.clone().to_xdr(&env));
        preimage.append(&target.clone().to_xdr(&env));
        preimage.append(&Bytes::from_array(&env, &eta.to_be_bytes()));
        let predicted_op_id: Bytes = env.crypto().sha256(&preimage).into();

        // Try to queue an operation with itself as a dependency
        let mut self_dep = Vec::new(&env);
        self_dep.push_back(predicted_op_id.clone());

        let result = client.try_queue(&admin, &desc, &target, &delay, &GRACE, &self_dep);
        assert_eq!(
            result,
            Err(Ok(TimelockError::CircularDependency)),
            "queue should reject circular self-dependency"
        );

        // Verify the operation was NOT stored
        let stored_op = client.get_op(&predicted_op_id);
        assert!(
            stored_op.is_none(),
            "circular dependency should prevent operation from being stored"
        );
    }

    #[test]
    fn test_queue_multi_hop_dependency_succeeds() {
        // This test verifies that a valid two-hop dependency chain (op1 ← op2)
        // is accepted by queue(). It does NOT test cycle rejection — a genuine
        // two-node cycle (op1 depends on op2 AND op2 depends on op1) cannot be
        // constructed through the public API: deps can only reference already-queued
        // op_ids, and there is no way to add a dependency to an operation after it
        // has been queued. The only feasible cycle is a self-dependency (op references
        // its own predicted id), which is covered by test_queue_circular_dependency_fails.
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let delay: u64 = 3600;

        // Queue first operation (no dependencies)
        let op1_id = client.queue(
            &admin,
            &String::from_str(&env, "op_1"),
            &target,
            &delay,
            &GRACE,
            &Vec::new(&env),
        );

        // Queue second operation that depends on the first — a valid chain
        let mut deps = Vec::new(&env);
        deps.push_back(op1_id.clone());

        let op2_id = client.queue(
            &admin,
            &String::from_str(&env, "op_2"),
            &target,
            &delay,
            &GRACE,
            &deps,
        );

        // Verify both operations are stored
        assert!(client.get_op(&op1_id).is_some(), "op1 should be stored");
        assert!(client.get_op(&op2_id).is_some(), "op2 should be stored");

        // Verify dependencies are recorded correctly
        let op2_deps = client.get_dependencies(&op2_id);
        assert_eq!(op2_deps.len(), 1, "op2 should have one dependency");
        assert_eq!(op2_deps.get(0).unwrap(), op1_id, "op2 should depend on op1");
    }

    #[test]
    fn test_dependency_chain_too_deep_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);

        // Build a chain of 10 ops: check_dependency_depth starts at depth 0 for the
        // immediate dependency, so a 9-node ancestor chain only reaches depth 8 (allowed);
        // a 10th ancestor pushes the walk to depth 9, which exceeds MAX_DEPENDENCY_DEPTH.
        let mut prev_deps: Vec<Bytes> = Vec::new(&env);
        let mut last_id = Bytes::from_array(&env, &[0u8; 32]);
        for i in 0..10u32 {
            let desc = std::format!("op_{}", i);
            let op_id = client.queue(
                &admin,
                &String::from_str(&env, &desc),
                &target,
                &3600,
                &GRACE,
                &prev_deps,
            );
            prev_deps = Vec::new(&env);
            prev_deps.push_back(op_id.clone());
            last_id = op_id;
        }

        // The 10th op depends on the 9-level chain: this must exceed MAX_DEPENDENCY_DEPTH
        let result = client.try_queue(
            &admin,
            &String::from_str(&env, "too_deep"),
            &target,
            &3600,
            &GRACE,
            &prev_deps,
        );
        assert_eq!(result, Err(Ok(TimelockError::DependencyTooDeep)));
        let _ = last_id;
    }

    // ── get_dependencies ──────────────────────────────────────────────────────

    #[test]
    fn test_get_dependencies_returns_stored_deps() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);

        // Queue a parent op (no deps)
        let parent_id = client.queue(
            &admin,
            &String::from_str(&env, "parent"),
            &target,
            &3600,
            &GRACE,
            &Vec::new(&env),
        );

        // Queue a child op that depends on the parent
        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(
            &admin,
            &String::from_str(&env, "child"),
            &target,
            &3600,
            &GRACE,
            &deps,
        );

        let stored_deps = client.get_dependencies(&child_id);
        assert_eq!(stored_deps.len(), 1);
        assert_eq!(stored_deps.get(0).unwrap(), parent_id);
    }

    #[test]
    fn test_get_dependencies_returns_empty_for_op_with_no_deps() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);

        let op_id = client.queue(
            &admin,
            &String::from_str(&env, "no deps"),
            &target,
            &3600,
            &GRACE,
            &Vec::new(&env),
        );

        let deps = client.get_dependencies(&op_id);
        assert_eq!(deps.len(), 0);
    }

    #[test]
    fn test_get_dependencies_returns_empty_for_nonexistent_op() {
        let (env, _admin, client) = setup();
        let fake_id = Bytes::from_array(&env, &[0u8; 32]);
        let deps = client.get_dependencies(&fake_id);
        assert_eq!(deps.len(), 0);
    }

    // ── Grace period overflow / cap tests ─────────────────────────────────────

    /// queue() must reject a grace_period_seconds value that exceeds
    /// MAX_GRACE_PERIOD_SECONDS (30 days = 2_592_000 s).
    #[test]
    fn test_queue_rejects_grace_period_exceeding_max() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "long grace");
        let deps: Vec<Bytes> = Vec::new(&env);

        // One second over the 30-day cap must be rejected.
        let over_cap: u64 = 30 * 24 * 60 * 60 + 1;
        let result = client.try_queue(&admin, &desc, &target, &3600, &over_cap, &deps);
        assert_eq!(result, Err(Ok(TimelockError::GracePeriodTooLong)));
    }

    /// u64::MAX as grace_period_seconds must be rejected (overflow safety).
    #[test]
    fn test_queue_rejects_u64_max_grace_period() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "u64 max grace");
        let deps: Vec<Bytes> = Vec::new(&env);

        let result = client.try_queue(&admin, &desc, &target, &3600, &u64::MAX, &deps);
        assert_eq!(result, Err(Ok(TimelockError::GracePeriodTooLong)));
    }

    /// queue() must accept a grace_period_seconds value exactly equal to
    /// MAX_GRACE_PERIOD_SECONDS (boundary inclusive).
    #[test]
    fn test_queue_accepts_grace_period_at_exact_max() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "exact max grace");
        let deps: Vec<Bytes> = Vec::new(&env);

        let max_grace: u64 = 30 * 24 * 60 * 60; // 2_592_000
        let result = client.try_queue(&admin, &desc, &target, &3600, &max_grace, &deps);
        assert!(
            result.is_ok(),
            "grace_period_seconds == MAX_GRACE_PERIOD_SECONDS must be accepted"
        );
    }

    /// queue() must accept a grace_period_seconds of zero (no expiry window —
    /// operation expires immediately after eta).
    #[test]
    fn test_queue_accepts_zero_grace_period() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "zero grace");
        let deps: Vec<Bytes> = Vec::new(&env);

        let result = client.try_queue(&admin, &desc, &target, &3600, &0, &deps);
        assert!(result.is_ok(), "grace_period_seconds == 0 must be accepted");
    }

    /// execute() must return Expired when now > eta + grace_period_seconds,
    /// confirming checked_add works correctly for normal (non-overflow) values.
    #[test]
    fn test_execute_rejects_after_grace_period_with_checked_expiry() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "expiry check");
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 7200; // 2-hour window

        let op_id = client.queue(&admin, &desc, &target, &3600, &grace, &deps);

        // Jump exactly 1 second past eta + grace_period_seconds.
        env.ledger().with_mut(|l| l.timestamp += 3600 + grace + 1);

        let result = client.try_execute(&admin, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::Expired)));
    }

    /// get_operation_status must return Expired for an op whose
    /// eta + grace_period_seconds has elapsed (uses checked_add path).
    #[test]
    fn test_get_operation_status_expired_uses_checked_add() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "status expiry");
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        let op_id = client.queue(&admin, &desc, &target, &3600, &grace, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3600 + grace + 1);

        assert_eq!(
            client.get_operation_status(&op_id),
            Some(OperationStatus::Expired)
        );
    }

    /// get_operation_status must treat an overflowing eta + grace_period_seconds
    /// as Expired, matching cleanup_expired and the other status functions.
    #[test]
    fn test_get_operation_status_overflowing_expiry_is_expired() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "overflow expiry");
        let op_id = Bytes::from_array(&env, &[0xAB; 32]);

        // queue() rejects such values, so arrange the anomalous stored op directly.
        env.as_contract(&client.address, || {
            let op = Op {
                proposer: admin.clone(),
                description: desc.clone(),
                target: target.clone(),
                eta: u64::MAX,
                grace_period_seconds: 1,
                executed: false,
                cancelled: false,
            };
            env.storage()
                .instance()
                .set(&DataKey::Op(op_id.clone()), &op);
        });

        assert_eq!(
            client.get_operation_status(&op_id),
            Some(OperationStatus::Expired)
        );
    }

    /// cleanup_expired must remove an op once eta + grace_period_seconds has
    /// elapsed, confirming the checked_add path in cleanup works correctly.
    #[test]
    fn test_cleanup_expired_uses_checked_add_for_expiry() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        client.queue(
            &admin,
            &String::from_str(&env, "will expire"),
            &target,
            &3600,
            &grace,
            &deps,
        );

        // Advance past eta + grace (3600 + 3600 + 1).
        env.ledger().with_mut(|l| l.timestamp += 7201);

        let cleaned = client.cleanup_expired(&admin, &10);
        assert_eq!(cleaned, 1);
        assert_eq!(client.get_pending_operations().len(), 0);
    }

    /// get_pending_operations must exclude an op whose checked expiry has
    /// elapsed, confirming the checked_add path in get_pending_operations.
    #[test]
    fn test_get_pending_operations_excludes_checked_expired_ops() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        // Queue one that will expire and one that won't.
        client.queue(
            &admin,
            &String::from_str(&env, "short grace"),
            &target,
            &3600,
            &grace,
            &deps,
        );
        client.queue(
            &admin,
            &String::from_str(&env, "long grace"),
            &target,
            &3600,
            &(30 * 24 * 60 * 60), // 30-day grace, well within cap
            &deps,
        );

        // Advance past the first op's grace period but not the second's.
        env.ledger().with_mut(|l| l.timestamp += 3600 + grace + 1);

        let pending = client.get_pending_operations();
        assert_eq!(
            pending.len(),
            1,
            "only the long-grace op should remain pending"
        );
    }

    /// get_operation_count_by_status must correctly count Expired ops using
    /// the checked_add path.
    #[test]
    fn test_get_operation_count_by_status_expired_uses_checked_add() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        client.queue(
            &admin,
            &String::from_str(&env, "op a"),
            &target,
            &3600,
            &grace,
            &deps,
        );
        client.queue(
            &admin,
            &String::from_str(&env, "op b"),
            &target,
            &3600,
            &grace,
            &deps,
        );

        // Both ops expire at the same time.
        env.ledger().with_mut(|l| l.timestamp += 3600 + grace + 1);

        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Expired),
            2
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Ready),
            0
        );
    }

    /// A grace period of exactly MAX - 1 seconds must be accepted and the op
    /// must execute at eta and expire 1 second before the cap.
    #[test]
    fn test_queue_one_below_max_grace_period_is_valid() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "one below max");
        let deps: Vec<Bytes> = Vec::new(&env);

        let grace: u64 = 30 * 24 * 60 * 60 - 1; // MAX_GRACE_PERIOD_SECONDS - 1
        let op_id = client.queue(&admin, &desc, &target, &3600, &grace, &deps);

        // Should be Queued right after creation.
        assert_eq!(
            client.get_operation_status(&op_id),
            Some(OperationStatus::Queued)
        );

        // Should become Ready after the delay.
        env.ledger().with_mut(|l| l.timestamp += 3601);
        assert_eq!(
            client.get_operation_status(&op_id),
            Some(OperationStatus::Ready)
        );

        // Should execute successfully.
        client.execute(&admin, &op_id);
        assert!(client.get_op(&op_id).unwrap().executed);
    }

    // ── Issue #1386: execute() on an op whose dependency was cancelled ────────

    /// execute() must return DependencyNotExecuted when a dependency was
    /// cancelled (not just pending). This covers the Blocked state transition
    /// that has_blocked_dependency / require_dependencies_executed are
    /// specifically designed to handle.
    #[test]
    fn test_execute_blocked_by_cancelled_dependency_returns_dependency_not_executed() {
        let (env, admin, client) = setup();
        let parent_target = Address::generate(&env);
        let child_target = Address::generate(&env);
        let no_deps: Vec<Bytes> = Vec::new(&env);

        // Queue the parent operation.
        let parent_id = client.queue(
            &admin,
            &String::from_str(&env, "register adapter"),
            &parent_target,
            &3600,
            &GRACE,
            &no_deps,
        );

        // Queue the child that depends on the parent.
        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(
            &admin,
            &String::from_str(&env, "upgrade adapter"),
            &child_target,
            &3600,
            &GRACE,
            &deps,
        );

        // Cancel the parent — the child's dependency can never be executed.
        client.cancel(&admin, &parent_id);

        // Advance time past the child's ETA so it would otherwise be Ready.
        env.ledger().with_mut(|l| l.timestamp += 3601);

        // The child is permanently Blocked; execute must return DependencyNotExecuted.
        let result = client.try_execute(&admin, &child_id);
        assert_eq!(result, Err(Ok(TimelockError::DependencyNotExecuted)));

        // Status must reflect Blocked, not Ready.
        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Blocked)
        );
    }

    /// Confirms the blocked state is permanent: even after additional time
    /// passes, execute() continues to return DependencyNotExecuted and the
    /// status remains Blocked (within the grace period).
    #[test]
    fn test_execute_blocked_by_cancelled_dependency_is_permanent() {
        let (env, admin, client) = setup();
        let parent_target = Address::generate(&env);
        let child_target = Address::generate(&env);
        let no_deps: Vec<Bytes> = Vec::new(&env);

        let parent_id = client.queue(
            &admin,
            &String::from_str(&env, "register adapter"),
            &parent_target,
            &3600,
            &GRACE,
            &no_deps,
        );

        let mut deps = Vec::new(&env);
        deps.push_back(parent_id.clone());
        let child_id = client.queue(
            &admin,
            &String::from_str(&env, "upgrade adapter"),
            &child_target,
            &3600,
            &GRACE,
            &deps,
        );

        // Cancel the parent before it is executed.
        client.cancel(&admin, &parent_id);

        // First check: right after the child's ETA.
        env.ledger().with_mut(|l| l.timestamp += 3601);
        assert_eq!(
            client.try_execute(&admin, &child_id),
            Err(Ok(TimelockError::DependencyNotExecuted))
        );
        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Blocked)
        );

        // Second check: well into the grace period — still permanently blocked.
        env.ledger().with_mut(|l| l.timestamp += GRACE / 2);
        assert_eq!(
            client.try_execute(&admin, &child_id),
            Err(Ok(TimelockError::DependencyNotExecuted))
        );
        assert_eq!(
            client.get_operation_status(&child_id),
            Some(OperationStatus::Blocked)
        );
    }
}
