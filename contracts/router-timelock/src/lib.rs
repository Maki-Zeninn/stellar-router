#![no_std]

//! # router-timelock
//!
//! Delayed execution queue for sensitive router configuration changes.
//! Any proposed change must wait a configurable delay before it can be executed.
//!
//! ## Features
//! - Queue arbitrary change proposals with a description
//! - Configurable minimum delay (e.g. 24h)
//! - Cancel queued operations before execution
//! - Executed operations cannot be re-executed

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, Address, Env, String, Symbol};

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Admin,
    MinDelay,
    Operation(u64),   // op_id -> TimelockOp
    NextOpId,
    OperationDeps(u64), // op_id -> Vec<u64> (dependencies)
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TimelockOp {
    pub id: u64,
    pub proposer: Address,
    pub description: String,
    /// Target contract address for the change
    pub target: Address,
    /// Earliest timestamp at which this op can execute
    pub eta: u64,
    pub executed: bool,
    pub cancelled: bool,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TimelockError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    NotFound = 4,
    TooEarly = 5,
    AlreadyExecuted = 6,
    AlreadyCancelled = 7,
    InvalidDelay = 8,
    DependencyNotMet = 9,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RouterTimelock;

#[contractimpl]
impl RouterTimelock {
    /// Initialize with admin and minimum delay in seconds.
    ///
    /// Must be called exactly once. Sets the admin, the minimum required delay
    /// for all queued operations, and initializes the operation ID counter.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `admin` - The address that will have admin privileges over this timelock.
    /// * `min_delay` - The minimum number of seconds that must elapse between
    ///   queuing and executing an operation. Must be greater than zero.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`TimelockError::AlreadyInitialized`] — if the contract has already been initialized.
    /// * [`TimelockError::InvalidDelay`] — if `min_delay` is zero.
    pub fn initialize(env: Env, admin: Address, min_delay: u64) -> Result<(), TimelockError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(TimelockError::AlreadyInitialized);
        }
        if min_delay == 0 {
            return Err(TimelockError::InvalidDelay);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::MinDelay, &min_delay);
        env.storage().instance().set(&DataKey::NextOpId, &0u64);
        Ok(())
    }

    /// Queue a new operation. Returns the operation ID.
    ///
    /// Creates a new [`TimelockOp`] with an ETA of `current_timestamp + delay`.
    /// The `delay` must be at least the configured `min_delay`. Caller must be
    /// the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `proposer` - The address proposing the operation; must be the admin.
    /// * `description` - A human-readable description of the proposed change.
    /// * `target` - The contract address that will be affected by the change.
    /// * `delay` - Number of seconds to wait before the operation can execute.
    ///   Must be >= the configured `min_delay`.
    /// * `depends_on` - Optional vector of operation IDs that must complete before this one.
    ///
    /// # Returns
    /// The `u64` operation ID assigned to the new operation.
    ///
    /// # Errors
    /// * [`TimelockError::Unauthorized`] — if `proposer` is not the admin.
    /// * [`TimelockError::InvalidDelay`] — if `delay` is less than `min_delay`.
    /// * [`TimelockError::NotInitialized`] — if the contract has not been initialized.
    pub fn queue(
        env: Env,
        proposer: Address,
        description: String,
        target: Address,
        delay: u64,
        depends_on: soroban_sdk::Vec<u64>,
    ) -> Result<u64, TimelockError> {
        proposer.require_auth();
        Self::require_admin(&env, &proposer)?;

        let min_delay: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MinDelay)
            .ok_or(TimelockError::NotInitialized)?;

        if delay < min_delay {
            return Err(TimelockError::InvalidDelay);
        }

        let op_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextOpId)
            .unwrap_or(0);

        let eta = env.ledger().timestamp() + delay;

        let op = TimelockOp {
            id: op_id,
            proposer,
            description,
            target,
            eta,
            executed: false,
            cancelled: false,
        };

        env.storage().instance().set(&DataKey::Operation(op_id), &op);
        if !depends_on.is_empty() {
            env.storage()
                .instance()
                .set(&DataKey::OperationDeps(op_id), &depends_on);
        }
        env.storage().instance().set(&DataKey::NextOpId, &(op_id + 1));

        Ok(op_id)
    }

    /// Execute a queued operation after its delay has elapsed.
    ///
    /// Marks the operation as executed. The current ledger timestamp must be
    /// >= the operation's ETA. The operation must not have been previously
    /// executed or cancelled. All dependencies must have been executed.
    /// Caller must be the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `op_id` - The ID of the operation to execute.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`TimelockError::Unauthorized`] — if `caller` is not the admin.
    /// * [`TimelockError::NotFound`] — if no operation with `op_id` exists.
    /// * [`TimelockError::AlreadyExecuted`] — if the operation has already been executed.
    /// * [`TimelockError::AlreadyCancelled`] — if the operation has been cancelled.
    /// * [`TimelockError::TooEarly`] — if the current timestamp is before the operation's ETA.
    /// * [`TimelockError::DependencyNotMet`] — if any dependency has not been executed.
    pub fn execute(env: Env, caller: Address, op_id: u64) -> Result<(), TimelockError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let mut op: TimelockOp = env
            .storage()
            .instance()
            .get(&DataKey::Operation(op_id))
            .ok_or(TimelockError::NotFound)?;

        if op.executed {
            return Err(TimelockError::AlreadyExecuted);
        }
        if op.cancelled {
            return Err(TimelockError::AlreadyCancelled);
        }
        if env.ledger().timestamp() < op.eta {
            return Err(TimelockError::TooEarly);
        }

        // Check dependencies
        if let Some(deps) = env
            .storage()
            .instance()
            .get::<DataKey, soroban_sdk::Vec<u64>>(&DataKey::OperationDeps(op_id))
        {
            for dep_id in deps.iter() {
                let dep_op: TimelockOp = env
                    .storage()
                    .instance()
                    .get(&DataKey::Operation(dep_id))
                    .ok_or(TimelockError::NotFound)?;
                if !dep_op.executed {
                    return Err(TimelockError::DependencyNotMet);
                }
            }
        }

        op.executed = true;
        env.storage().instance().set(&DataKey::Operation(op_id), &op);

        Ok(())
    }

    /// Cancel a queued operation before it executes.
    ///
    /// Marks the operation as cancelled, preventing future execution. The
    /// operation must not have been previously executed or cancelled. Caller
    /// must be the admin. Dependencies are cleared but can be cancelled
    /// independently. The operation's state is fully cleared to prevent
    /// re-execution if re-queued.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `op_id` - The ID of the operation to cancel.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`TimelockError::Unauthorized`] — if `caller` is not the admin.
    /// * [`TimelockError::NotFound`] — if no operation with `op_id` exists.
    /// * [`TimelockError::AlreadyExecuted`] — if the operation has already been executed.
    /// * [`TimelockError::AlreadyCancelled`] — if the operation has already been cancelled.
    pub fn cancel(env: Env, caller: Address, op_id: u64) -> Result<(), TimelockError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let mut op: TimelockOp = env
            .storage()
            .instance()
            .get(&DataKey::Operation(op_id))
            .ok_or(TimelockError::NotFound)?;

        if op.executed {
            return Err(TimelockError::AlreadyExecuted);
        }
        if op.cancelled {
            return Err(TimelockError::AlreadyCancelled);
        }

        op.cancelled = true;
        env.storage().instance().set(&DataKey::Operation(op_id), &op);
        // Clear dependencies when cancelling
        env.storage().instance().remove(&DataKey::OperationDeps(op_id));

        Ok(())
    }

    /// Cancel all pending operations.
    ///
    /// Iterates through all queued operations and marks those that have not yet
    /// been executed or cancelled as cancelled. This is an emergency function
    /// that can only be called by the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `admin` - The address initiating the call; must be the admin.
    ///
    /// # Returns
    /// The number of operations that were successfully cancelled.
    ///
    /// # Errors
    /// * [`TimelockError::Unauthorized`] — if `admin` is not the authorized admin.
    /// * [`TimelockError::NotInitialized`] — if the contract has not been initialized.
    pub fn cancel_all(env: Env, admin: Address) -> Result<u32, TimelockError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;

        let next_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextOpId)
            .unwrap_or(0);

        let mut count = 0u32;
        for id in 0..next_id {
            let key = DataKey::Operation(id);
            if let Some(mut op) = env.storage().instance().get::<DataKey, TimelockOp>(&key) {
                if !op.executed && !op.cancelled {
                    op.cancelled = true;
                    env.storage().instance().set(&key, &op);
                    count += 1;

                    env.events().publish(
                        (Symbol::new(&env, "cancel"), id),
                        (op.description.clone(), op.target.clone()),
                    );
                }
            }
        }
        Ok(count)
    }

    /// Get an operation by ID.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `op_id` - The ID of the operation to retrieve.
    ///
    /// # Returns
    /// `Some(`[`TimelockOp`]`)` if the operation exists, `None` otherwise.
    pub fn get_op(env: Env, op_id: u64) -> Option<TimelockOp> {
        env.storage().instance().get(&DataKey::Operation(op_id))
    }

    /// Get the minimum delay.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// The minimum delay in seconds that must be used when queuing operations.
    ///
    /// # Errors
    /// * [`TimelockError::NotInitialized`] — if the contract has not been initialized.
    pub fn min_delay(env: Env) -> Result<u64, TimelockError> {
        env.storage()
            .instance()
            .get(&DataKey::MinDelay)
            .ok_or(TimelockError::NotInitialized)
    }

    /// Update the minimum delay.
    ///
    /// Changes the minimum required delay for future queued operations. Does
    /// not affect already-queued operations. Caller must be the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `new_delay` - The new minimum delay in seconds. Must be greater than zero.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`TimelockError::Unauthorized`] — if `caller` is not the admin.
    /// * [`TimelockError::InvalidDelay`] — if `new_delay` is zero.
    /// * [`TimelockError::NotInitialized`] — if the contract has not been initialized.
    pub fn set_min_delay(env: Env, caller: Address, new_delay: u64) -> Result<(), TimelockError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        if new_delay == 0 {
            return Err(TimelockError::InvalidDelay);
        }
        env.storage().instance().set(&DataKey::MinDelay, &new_delay);
        Ok(())
    }

    /// Get current admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// The [`Address`] of the current admin.
    ///
    /// # Errors
    /// * [`TimelockError::NotInitialized`] — if the contract has not been initialized.
    pub fn admin(env: Env) -> Result<Address, TimelockError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(TimelockError::NotInitialized)
    }

    /// Transfer admin to a new address.
    ///
    /// Replaces the current admin with `new_admin`. The `current` address must
    /// authenticate and must be the existing admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `current` - The current admin address; must authenticate.
    /// * `new_admin` - The address that will become the new admin.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`TimelockError::Unauthorized`] — if `current` is not the admin.
    /// * [`TimelockError::NotInitialized`] — if the contract has not been initialized.
    pub fn transfer_admin(env: Env, current: Address, new_admin: Address) -> Result<(), TimelockError> {
        current.require_auth();
        Self::require_admin(&env, &current)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
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
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Ledger}, Env, String};

    fn setup() -> (Env, Address, RouterTimelockClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1000);
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &3600);
        (env, admin, client)
    }

    #[test]
    fn test_queue_and_execute() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        let result = client.try_execute(&admin, &op_id);
        assert!(result.is_ok());
        let op = client.get_op(&op_id).unwrap();
        assert!(op.executed);
    }

    #[test]
    fn test_execute_too_early_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        let result = client.try_execute(&admin, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::TooEarly)));
    }

    #[test]
    fn test_cancel_operation() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        client.cancel(&admin, &op_id);
        let op = client.get_op(&op_id).unwrap();
        assert!(op.cancelled);
    }

    #[test]
    fn test_execute_cancelled_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        client.cancel(&admin, &op_id);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        let result = client.try_execute(&admin, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::AlreadyCancelled)));
    }

    #[test]
    fn test_double_execute_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);
        let result = client.try_execute(&admin, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::AlreadyExecuted)));
    }

    #[test]
    fn test_delay_below_minimum_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);
        let result = client.try_queue(&admin, &desc, &target, &100, &deps);
        assert_eq!(result, Err(Ok(TimelockError::InvalidDelay)));
    }

    #[test]
    fn test_unauthorized_queue_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "malicious");
        let deps = soroban_sdk::Vec::new(&env);
        let result = client.try_queue(&attacker, &desc, &target, &3600, &deps);
        assert_eq!(result, Err(Ok(TimelockError::Unauthorized)));
    }

    #[test]
    fn test_cancel_all_no_pending() {
        let (_env, admin, client) = setup();
        let cancelled = client.cancel_all(&admin);
        assert_eq!(cancelled, 0);
    }

    #[test]
    fn test_cancel_all_one_pending() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);
        client.queue(&admin, &desc, &target, &3600, &deps);
        let cancelled = client.cancel_all(&admin);
        assert_eq!(cancelled, 1);
        let op = client.get_op(&0).unwrap();
        assert!(op.cancelled);
    }

    #[test]
    fn test_cancel_all_multiple_pending() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);

        client.queue(&admin, &desc, &target, &3600, &deps);
        client.queue(&admin, &desc, &target, &3600, &deps);
        client.queue(&admin, &desc, &target, &3600, &deps);

        // Mark one as executed
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &0);

        // Mark one as already cancelled
        client.cancel(&admin, &1);

        // Remaining: 1 pending
        let cancelled = client.cancel_all(&admin);
        assert_eq!(cancelled, 1);

        let op0 = client.get_op(&0).unwrap();
        assert!(op0.executed);

        let op1 = client.get_op(&1).unwrap();
        assert!(op1.cancelled);

        let op2 = client.get_op(&2).unwrap();
        assert!(op2.cancelled);
    }

    #[test]
    fn test_admin_getter() {
        let (env, admin, client) = setup();
        let retrieved_admin = client.admin();
        assert_eq!(retrieved_admin, admin);
    }

    #[test]
    fn test_transfer_admin() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);
        assert_eq!(client.admin(), new_admin);
    }

    #[test]
    fn test_unauthorized_transfer_admin_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let result = client.try_transfer_admin(&attacker, &new_admin);
        assert_eq!(result, Err(Ok(TimelockError::Unauthorized)));
    }

    #[test]
    fn test_set_min_delay_updates_value() {
        let (env, admin, client) = setup();
        client.set_min_delay(&admin, &7200);
        assert_eq!(client.min_delay(), 7200);
    }

    #[test]
    fn test_set_min_delay_zero_fails() {
        let (env, admin, client) = setup();
        let result = client.try_set_min_delay(&admin, &0);
        assert_eq!(result, Err(Ok(TimelockError::InvalidDelay)));
    }

    #[test]
    fn test_set_min_delay_unauthorized_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let result = client.try_set_min_delay(&attacker, &7200);
        assert_eq!(result, Err(Ok(TimelockError::Unauthorized)));
    }

    #[test]
    fn test_set_min_delay_does_not_affect_existing_ops() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);
        // Queue with current min_delay of 3600
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        // Raise min_delay to 7200 — op was already queued with delay=3600
        client.set_min_delay(&admin, &7200);
        // Advance past the original ETA
        env.ledger().with_mut(|l| l.timestamp += 3601);
        // Execute must still succeed — the op was valid when queued
        assert!(client.try_execute(&admin, &op_id).is_ok());
    }

    #[test]
    fn test_old_admin_locked_out_after_transfer() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);

        // old admin should no longer be able to call admin-only functions
        let result = client.try_set_min_delay(&admin, &7200);
        assert_eq!(result, Err(Ok(TimelockError::Unauthorized)));

        // new admin should be able to update min delay
        assert!(client.try_set_min_delay(&new_admin, &7200).is_ok());
        assert_eq!(client.min_delay(), 7200);
    }

    #[test]
    fn test_operation_with_dependencies() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);

        // Queue op0 with no dependencies
        let op0 = client.queue(&admin, &desc, &target, &3600, &deps);

        // Queue op1 that depends on op0
        let mut deps_for_op1 = soroban_sdk::Vec::new(&env);
        deps_for_op1.push_back(op0);
        let op1 = client.queue(&admin, &desc, &target, &3600, &deps_for_op1);

        // Try to execute op1 before op0 — should fail
        env.ledger().with_mut(|l| l.timestamp += 3601);
        let result = client.try_execute(&admin, &op1);
        assert_eq!(result, Err(Ok(TimelockError::DependencyNotMet)));

        // Execute op0 first
        assert!(client.try_execute(&admin, &op0).is_ok());

        // Now op1 should execute
        assert!(client.try_execute(&admin, &op1).is_ok());
    }

    #[test]
    fn test_cancelled_operation_clears_dependencies() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = soroban_sdk::Vec::new(&env);

        let op0 = client.queue(&admin, &desc, &target, &3600, &deps);

        let mut deps_for_op1 = soroban_sdk::Vec::new(&env);
        deps_for_op1.push_back(op0);
        let op1 = client.queue(&admin, &desc, &target, &3600, &deps_for_op1);

        // Cancel op1 — dependencies should be cleared
        client.cancel(&admin, &op1);

        // op0 can still be cancelled independently
        assert!(client.try_cancel(&admin, &op0).is_ok());
    }

    #[test]
    fn test_requeued_operation_has_fresh_eta() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle to v2");
        let deps = soroban_sdk::Vec::new(&env);

        // Queue operation with delay 86400
        let op_id_1 = client.queue(&admin, &desc, &target, &86400, &deps);
        let op_1 = client.get_op(&op_id_1).unwrap();
        let eta_1 = op_1.eta;

        // Cancel it immediately
        client.cancel(&admin, &op_id_1);

        // Advance time by 1 second
        env.ledger().with_mut(|l| l.timestamp += 1);

        // Re-queue with identical description and target
        let op_id_2 = client.queue(&admin, &desc, &target, &86400, &deps);
        let op_2 = client.get_op(&op_id_2).unwrap();
        let eta_2 = op_2.eta;

        // New operation should have a different ID
        assert_ne!(op_id_1, op_id_2);

        // New operation should have a fresh ETA (1 second later)
        assert_eq!(eta_2, eta_1 + 1);

        // Verify old operation is still cancelled
        let old_op = client.get_op(&op_id_1).unwrap();
        assert!(old_op.cancelled);

        // Verify new operation is not cancelled
        assert!(!op_2.cancelled);

        // Try to execute old operation — should fail
        env.ledger().with_mut(|l| l.timestamp += 86400);
        let result = client.try_execute(&admin, &op_id_1);
        assert_eq!(result, Err(Ok(TimelockError::AlreadyCancelled)));

        // New operation should still be too early (needs 86400 from its own queue time)
        let result = client.try_execute(&admin, &op_id_2);
        assert_eq!(result, Err(Ok(TimelockError::TooEarly)));

        // Advance to new operation's ETA
        env.ledger().with_mut(|l| l.timestamp += 1);
        assert!(client.try_execute(&admin, &op_id_2).is_ok());
    }
}
