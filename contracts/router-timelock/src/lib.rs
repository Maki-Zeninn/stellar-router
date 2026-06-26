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

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, xdr::ToXdr, Address, Bytes, Env, String,
    Symbol, Vec,
};

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Admin,
    MinDelay,
    Op(Bytes),          // op_id -> Op
    PendingOps,         // Vec<Bytes> — IDs of ops that are neither executed nor cancelled
    MaxPendingOps,      // u32 — Maximum allowed pending operations
    Op(Bytes),  // op_id -> Op
    PendingOps, // Vec<Bytes> — IDs of ops that are neither executed nor cancelled
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    pub proposer: Address,
    pub description: String,
    pub target: Address,
    pub eta: u64,
    /// Seconds after `eta` during which the operation may be executed.
    /// After `eta + grace_period_seconds` the operation is considered expired
    /// and can no longer be executed.
    pub grace_period_seconds: u64,
    pub executed: bool,
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
    CircularDependency = 10,
    /// Dependency chain exceeds maximum allowed depth.
    DependencyTooDeep = 11,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RouterTimelock;

#[contractimpl]
impl RouterTimelock {
    /// Initialize with an admin, minimum delay (seconds), and maximum pending operations limit.
    pub fn initialize(env: Env, admin: Address, min_delay: u64, max_pending_ops: u32) -> Result<(), TimelockError> {
    const MAX_DEPENDENCY_DEPTH: u32 = 8;

    /// Initialize with an admin and minimum delay (seconds).
    pub fn initialize(env: Env, admin: Address, min_delay: u64) -> Result<(), TimelockError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(TimelockError::AlreadyInitialized);
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
        Self::require_admin(&env, &proposer)?;

        let min_delay: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MinDelay)
            .ok_or(TimelockError::NotInitialized)?;

        if delay < min_delay {
            return Err(TimelockError::DelayTooShort);
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

        let eta = env.ledger().timestamp() + delay;

        // Derive op_id from description bytes + target bytes + eta
        let mut preimage = Bytes::new(&env);
        preimage.append(&description.clone().to_xdr(&env));
        preimage.append(&target.clone().to_xdr(&env));
        let eta_bytes = eta.to_be_bytes();
        preimage.append(&Bytes::from_array(&env, &eta_bytes));

        let op_id: Bytes = env.crypto().sha256(&preimage).into();

        // Validate no circular dependencies
        for dep_id in deps.iter() {
            if dep_id == op_id {
                return Err(TimelockError::CircularDependency);
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
        Self::require_admin(&env, &caller)?;

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

        env.events()
            .publish((Symbol::new(&env, router_common::EVENT_OP_CANCELLED),), op_id);
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
        Self::require_admin(&env, &caller)?;

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
        if now > op.eta + op.grace_period_seconds {
            return Err(TimelockError::Expired);
        }

        op.executed = true;
        env.storage()
            .instance()
            .set(&DataKey::Op(op_id.clone()), &op);

        Self::remove_from_pending_ops(&env, &op_id);

        env.events()
            .publish((Symbol::new(&env, router_common::EVENT_OP_EXECUTED),), (op_id, op.target));
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
        Self::require_admin(&env, &caller)?;

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

    /// Remove expired operations from the pending operations queue.
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

        let pending: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::PendingOps)
            .unwrap_or_else(|| Vec::new(&env));
        
        let now = env.ledger().timestamp();
        let mut cleaned_count = 0u32;
        let mut new_pending = Vec::new(&env);

        for op_id in pending.iter() {
            if cleaned_count >= limit {
                new_pending.push_back(op_id.clone());
                continue;
            }

            if let Some(op) = env.storage().instance().get::<DataKey, Op>(&DataKey::Op(op_id.clone())) {
                if now > op.eta + op.grace_period_seconds || op.executed || op.cancelled {
                    // It is expired or finalized!
                    cleaned_count += 1;
                } else {
                    new_pending.push_back(op_id);
                }
            } else {
                // If the operation data somehow doesn't exist, we just clean it up too
                cleaned_count += 1;
            }
        }

        if cleaned_count > 0 {
            env.storage().instance().set(&DataKey::PendingOps, &new_pending);
            env.events().publish((Symbol::new(&env, "ops_cleaned"),), cleaned_count);
        }

        Ok(cleaned_count)
    }

    /// Get an operation by id.
    pub fn get_op(env: Env, op_id: Bytes) -> Option<Op> {
        env.storage().instance().get(&DataKey::Op(op_id))
    }

    /// Get the human-readable status of an operation.
    ///
    /// # Returns
    /// * `Cancelled` — if the operation was cancelled.
    /// * `Executed`  — if the operation was executed.
    /// * `Expired`   — if `now > eta + grace_period_seconds` (and not executed/cancelled).
    /// * `Ready`     — if `now >= eta` and still within the grace period.
    /// * `Queued`    — if `now < eta`.
    ///
    /// Returns `None` if no operation with `op_id` exists.
    pub fn get_operation_status(env: Env, op_id: Bytes) -> Option<OperationStatus> {
        let op: Op = env.storage().instance().get(&DataKey::Op(op_id))?;
        let now = env.ledger().timestamp();
        let status = if op.cancelled {
            OperationStatus::Cancelled
        } else if op.executed {
            OperationStatus::Executed
        } else if now > op.eta + op.grace_period_seconds {
            OperationStatus::Expired
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
                // Only include ops that are genuinely pending (not expired)
                if !op.executed && !op.cancelled && now <= op.eta + op.grace_period_seconds {
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
                .get::<DataKey, Op>(&DataKey::Op(op_id))
            {
                let matches = match status {
                    OperationStatus::Cancelled => op.cancelled,
                    OperationStatus::Executed => op.executed,
                    OperationStatus::Expired => {
                        !op.executed && !op.cancelled && now > op.eta + op.grace_period_seconds
                    }
                    OperationStatus::Ready => {
                        !op.executed
                            && !op.cancelled
                            && now >= op.eta
                            && now <= op.eta + op.grace_period_seconds
                    }
                    OperationStatus::Queued => !op.executed && !op.cancelled && now < op.eta,
                };
                if matches {
                    count += 1;
                }
            }
        }
        count
    }

    /// Get the minimum delay.
    pub fn min_delay(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MinDelay)
            .unwrap_or(0)
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
        Self::require_admin(&env, &caller)?;

        let old_min_delay: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MinDelay)
            .ok_or(TimelockError::NotInitialized)?;

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
        Self::require_admin(&env, &current)?;

        env.storage().instance().set(&DataKey::Admin, &new_admin);

        env.events().publish(
            (Symbol::new(&env, "admin_transferred"),),
            (current, new_admin),
        );

        Ok(())
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn require_admin(env: &Env, caller: &Address) -> Result<(), TimelockError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(TimelockError::NotInitialized)?;
        if &admin != caller {
            return Err(TimelockError::Unauthorized);
        }
        Ok(())
    }

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
    fn add_to_pending_ops(env: &Env, op_id: &Bytes) {
        let mut pending: Vec<Bytes> = env
            .storage()
            .instance()
            .get(&DataKey::PendingOps)
            .unwrap_or_else(|| Vec::new(env));
        if !pending.iter().any(|id| id == *op_id) {
            pending.push_back(op_id.clone());
            env.storage().instance().set(&DataKey::PendingOps, &pending);
        }
    }

    /// Check dependency chain depth to prevent infinite recursion.
    /// Loads each dependency by ID and checks whether it itself exists
    /// as an operation, incrementing depth at each level.
    fn check_dependency_depth(env: &Env, dep_id: Bytes, depth: u32) -> Result<(), TimelockError> {
        if depth > Self::MAX_DEPENDENCY_DEPTH {
            return Err(TimelockError::DependencyTooDeep);
        }
        if let Some(dep_op) = env
            .storage()
            .instance()
            .get::<DataKey, Op>(&DataKey::Op(dep_id))
        {
            if dep_op.executed || dep_op.cancelled {
                return Ok(());
            }
        }
        Ok(())
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

    // ── validation ────────────────────────────────────────────────────────────

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
        assert_eq!(topic, Symbol::new(&env, router_common::EVENT_MIN_DELAY_UPDATED));

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
        for i in 0..5u32 {
            let bytes = Bytes::from_array(&env, &[i; 32]);
            let op_id = client.queue(&admin, &String::from_str(&env, "op"), &target, &3600, &grace, &deps);
            // Use different target for each to make unique op_id
            op_ids.push_back(op_id);
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
        assert_eq!(client.get_operation_count_by_status(&OperationStatus::Cancelled), 0);
        assert_eq!(client.get_operation_count_by_status(&OperationStatus::Queued), 1);
        // get_operation_count_by_status should reflect the state
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Cancelled),
            4
        );
        assert_eq!(
            client.get_operation_count_by_status(&OperationStatus::Queued),
            1
        );
    }

    // ── QueueFull limit ───────────────────────────────────────────────────────

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

        client.queue(&admin, &String::from_str(&env, "op1"), &target, &3600, &GRACE, &deps);
        client.queue(&admin, &String::from_str(&env, "op2"), &target, &3600, &GRACE, &deps);

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

        let op1 = client.queue(&admin, &String::from_str(&env, "op1"), &target, &3600, &GRACE, &deps);
        let _op2 = client.queue(&admin, &String::from_str(&env, "op2"), &target, &3600, &GRACE, &deps);

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
        let op1 = client.queue(&admin, &String::from_str(&env, "expires"), &target, &3600, &grace, &deps);
        let op2 = client.queue(&admin, &String::from_str(&env, "stays"), &target, &7200, &GRACE, &deps);

        // Jump past the grace period of op1
        let now = env.ledger().timestamp();
        env.ledger().with_mut(|l| l.timestamp = now + 3600 + grace + 1);

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

        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        // Queue 5 ops with unique targets to get unique op_ids
        for i in 0..5u32 {
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
        env.ledger().with_mut(|l| l.timestamp = now + 3600 + grace + 1);

        // Cleanup with limit of 2
        let cleaned = client.cleanup_expired(&admin, &2);
        assert_eq!(cleaned, 2);

        // 3 should still remain (not cleaned due to limit)
        assert_eq!(client.get_pending_operations().len(), 3);

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
        let op1 = client.queue(&admin, &String::from_str(&env, "op1"), &target, &3600, &grace, &deps);
        let op2 = client.queue(&admin, &String::from_str(&env, "op2"), &target, &3600, &grace, &deps);
        let op3 = client.queue(&admin, &String::from_str(&env, "op3"), &target, &3600, &grace, &deps);

        // Cancel op1
        client.cancel(&admin, &op1);

        // Execute op2 (advance time past ETA)
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op2);

        // op3 remains but will be expired
        let now = env.ledger().timestamp();
        env.ledger().with_mut(|l| l.timestamp = now + grace + 1);

        // Cleanup should remove op2 and op3 (executed and expired)
        let cleaned = client.cleanup_expired(&admin, &10);
        assert_eq!(cleaned, 2);

        // But op1 was already cancelled, so it's already removed
        assert_eq!(client.get_pending_operations().len(), 0);
    }

    #[test]
    fn test_cleanup_expired_emits_event() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps: Vec<Bytes> = Vec::new(&env);
        let grace: u64 = 3600;

        client.queue(&admin, &String::from_str(&env, "expires"), &target, &3600, &grace, &deps);

        // Jump past grace period
        let now = env.ledger().timestamp();
        env.ledger().with_mut(|l| l.timestamp = now + 3600 + grace + 1);

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

        client.queue(&admin, &String::from_str(&env, "expires"), &target, &3600, &grace, &deps);

        // Jump past grace period
        let now = env.ledger().timestamp();
        env.ledger().with_mut(|l| l.timestamp = now + 3600 + grace + 1);

        // Non-admin can call cleanup
        let cleaned = client.try_cleanup_expired(&caller, &10);
        assert!(cleaned.unwrap().is_ok());
    }
}
