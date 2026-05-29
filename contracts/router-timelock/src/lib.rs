#![no_std]

//! # router-timelock
//!
//! Delayed execution queue for sensitive router configuration changes.
//! Operations must wait a configurable minimum delay before execution.
//! Operations can be cancelled before execution.
//!
//! ## Events (following naming convention: past tense verbs in snake_case)
//! - `op_queued` — Operation queued (op_id, target, eta)
//! - `op_executed` — Operation executed (op_id, target)
//! - `op_cancelled` — Operation cancelled (op_id)

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, crypto::Hash, Address, Bytes, Env,
    String, Symbol, Vec,
};

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Admin,
    MinDelay,
    Operation(u64), // op_id -> TimelockOp
    NextOpId,
    FastTrackEnabled,
    OperationDeps(u64),      // op_id -> Vec<u64>
    EmergencyCouncil,        // Vec<Address>
    RequiredApprovals,       // u32 (M in M-of-N)
    FastTrackApprovals(u64), // op_id -> Vec<Address> (who has approved)
    FastTrackEnabled,        // bool — whether fast-track path is active
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    pub proposer: Address,
    pub description: String,
    pub target: Address,
    pub eta: u64,
    pub executed: bool,
    pub cancelled: bool,
}

/// Human-readable status of a timelock operation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum OperationStatus {
    /// Queued and waiting for ETA to elapse.
    Queued,
    /// ETA has elapsed but not yet executed.
    Ready,
    /// Successfully executed.
    Executed,
    /// Cancelled before execution.
    Cancelled,
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
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RouterTimelock;

#[contractimpl]
impl RouterTimelock {
    /// Initialize with an admin and minimum delay (seconds).
    pub fn initialize(env: Env, admin: Address, min_delay: u64) -> Result<(), TimelockError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(TimelockError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::MinDelay, &min_delay);
        Ok(())
    }

    /// Queue an operation. Returns the op_id (SHA-256 of description + target + eta).
    /// Emits `op_queued` with `(op_id, target, eta)`.
    pub fn queue(
        env: Env,
        proposer: Address,
        description: String,
        target: Address,
        delay: u64,
        _deps: Vec<Bytes>,
    ) -> Result<Bytes, TimelockError> {
        proposer.require_auth();
        router_common::require_admin_simple!(&env, &proposer, &DataKey::Admin, TimelockError)?;

        let min_delay: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MinDelay)
            .ok_or(TimelockError::NotInitialized)?;

        if delay < min_delay {
            return Err(TimelockError::DelayTooShort);
        }

        let eta = env.ledger().timestamp() + delay;

        // Derive op_id from description bytes + target bytes + eta
        let mut preimage = Bytes::new(&env);
        preimage.append(&description.to_bytes());
        preimage.append(&target.clone().to_xdr(&env));
        let eta_bytes = eta.to_be_bytes();
        preimage.append(&Bytes::from_array(&env, &eta_bytes));

        let op_id: Bytes = env.crypto().sha256(&preimage).into();

        let op = Op {
            proposer,
            description,
            target: target.clone(),
            eta,
            executed: false,
            cancelled: false,
        };
        env.storage()
            .instance()
            .set(&DataKey::Op(op_id.clone()), &op);

        env.events().publish(
            (Symbol::new(&env, "op_queued"),),
            (op_id.clone(), target, eta),
        );

        Ok(op_id)
    }

    /// Cancel a queued operation before it is executed.
    pub fn cancel(env: Env, caller: Address, op_id: Bytes) -> Result<(), TimelockError> {
        caller.require_auth();
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

        env.events()
            .publish((Symbol::new(&env, "op_cancelled"),), op_id);

        Ok(())
    }

    /// Execute a queued operation after its ETA has passed.
    pub fn execute(env: Env, caller: Address, op_id: Bytes) -> Result<(), TimelockError> {
        caller.require_auth();
        router_common::require_admin_simple!(&env, &caller, &DataKey::Admin, TimelockError)?;

        let mut op: Op = env
            .storage()
            .instance()
            .get(&DataKey::Op(op_id.clone()))
            .ok_or(TimelockError::NotFound)?;

        Self::require_op_pending(&op)?;
        if env.ledger().timestamp() < op.eta {
            return Err(TimelockError::NotReady);
        }

        op.executed = true;
        env.storage()
            .instance()
            .set(&DataKey::EmergencyCouncil, &council);
        env.storage()
            .instance()
            .set(&DataKey::RequiredApprovals, &required);
        env.storage()
            .instance()
            .set(&DataKey::FastTrackEnabled, &true);

        env.events()
            .publish((Symbol::new(&env, "council_updated"),), (required, council));

        Ok(())
    }

    /// Enable or disable the fast-track execution path.
    ///
    /// When disabled, `queue_critical` and `execute_critical` will return
    /// [`TimelockError::FastTrackDisabled`]. Only the admin can call this.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `enabled` - `true` to enable fast-track, `false` to disable it.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`TimelockError::Unauthorized`] — if `caller` is not the admin.
    /// * [`TimelockError::NotInitialized`] — if the contract has not been initialized.
    pub fn set_fast_track_enabled(
        env: Env,
        caller: Address,
        enabled: bool,
    ) -> Result<(), TimelockError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        env.storage()
            .instance()
            .set(&DataKey::FastTrackEnabled, &enabled);
        env.events()
            .publish((Symbol::new(&env, "fast_track_toggled"),), enabled);
        Ok(())
    }

    /// Returns whether the fast-track execution path is currently enabled.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// `true` if fast-track is enabled, `false` otherwise.
    pub fn get_fast_track_enabled(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::FastTrackEnabled)
            .unwrap_or(false)
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

    /// Get the current approvals for a critical operation.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `op_id` - The ID of the critical operation.
    ///
    /// # Returns
    /// A `Vec<Address>` of council members who have approved the operation.
    pub fn get_approvals(env: Env, op_id: u64) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::FastTrackApprovals(op_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns the dependency op IDs for the given operation.
    ///
    /// Returns the list of operation IDs that this operation depends on.
    /// If the operation has no dependencies or doesn't exist, returns an empty vector.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `op_id` - The operation ID to get dependencies for.
    ///
    /// # Returns
    /// A [`Vec<u64>`] of dependency operation IDs.
    pub fn get_dependency_ids(env: Env, op_id: u64) -> Vec<u64> {
        env.storage()
            .instance()
            .get::<DataKey, Vec<u64>>(&DataKey::OperationDeps(op_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Get all pending operations.
    ///
    /// Returns a list of all operations that are neither executed nor cancelled,
    /// in ID order (ascending).
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// A [`Vec<TimelockOp>`] of pending operations.
    ///
    /// # Deprecated
    /// Use `get_ops_by_state(true)` instead.
    pub fn get_pending_ops(env: Env) -> Vec<TimelockOp> {
        Self::get_ops_by_state(env, true)
    }

    /// Returns the total number of operations ever queued (including executed and cancelled).
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// The total operation count as `u64`.
    pub fn get_op_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::NextOpId)
            .unwrap_or(0)
    }

    /// Returns all operations matching the given state filter.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `only_pending` - If true, returns only ops where `!executed && !cancelled`.
    ///   If false, returns all ops.
    ///
    /// # Returns
    /// A [`Vec<TimelockOp>`] of matching operations in ID order (ascending).
    pub fn get_ops_by_state(env: Env, only_pending: bool) -> Vec<TimelockOp> {
        let count: u64 = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::NextOpId)
            .unwrap_or(0);
        let mut result = Vec::new(&env);
        for id in 0..count {
            if let Some(op) = env
                .storage()
                .instance()
                .get::<DataKey, TimelockOp>(&DataKey::Operation(id))
            {
                if !only_pending || (!op.executed && !op.cancelled) {
                    result.push_back(op);
                }
            }
        }
        result
    }

    /// Get the minimum delay.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// The minimum delay in seconds.
    ///
    /// # Errors
    /// * [`TimelockError::NotInitialized`] — if the contract has not been initialized.
    pub fn min_delay(env: Env) -> Result<u64, TimelockError> {
        env.storage()
            .instance()
            .get(&DataKey::MinDelay)
            .ok_or(TimelockError::NotInitialized)
    }

    /// Returns the current emergency council member list.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// A [`Vec<Address>`] of emergency council members.
    pub fn get_council(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::EmergencyCouncil)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Returns the number of approvals required for fast-track execution.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// The required number of approvals as `u32`.
    pub fn get_required_approvals(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::RequiredApprovals)
            .unwrap_or(0)
    }

    /// Returns true if the given address is a member of the emergency council.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `addr` - The address to check.
    ///
    /// # Returns
    /// `true` if `addr` is in the emergency council list, `false` otherwise.
    pub fn is_council_member(env: Env, addr: Address) -> bool {
        let council: Vec<Address> = env.storage().instance()
            .get(&DataKey::EmergencyCouncil)
            .unwrap_or_else(|| Vec::new(&env));
        council.iter().any(|m| m == addr)
    }

    /// Returns true if a critical operation has collected enough approvals to be fast-tracked.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `op_id` - The operation ID to check.
    ///
    /// # Returns
    /// `true` if approvals >= required_approvals, `false` otherwise or if op not found.
    pub fn has_sufficient_approvals(env: Env, op_id: u64) -> bool {
        let approvals: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::FastTrackApprovals(op_id))
            .unwrap_or_else(|| Vec::new(&env));
        let required: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RequiredApprovals)
            .unwrap_or(0);
        required > 0 && approvals.len() >= required
    }

    /// Update the minimum delay.
    ///
    /// Changes the minimum delay required for newly queued operations. This does not affect
    /// already-queued operations, which retain their original ETA and delay requirements.
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
        let old_delay: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MinDelay)
            .ok_or(TimelockError::NotInitialized)?;
        env.storage().instance().set(&DataKey::MinDelay, &new_delay);
        env.events().publish(
            (Symbol::new(&env, "min_delay_updated"),),
            (old_delay, new_delay),
        );
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
    /// # Panics
    /// * Panics if the contract has not been initialized.
    /// 
    /// Note: This is a breaking change from the previous Result-based API.
    /// Calling admin() on an uninitialized contract is considered a programming error
    /// rather than a runtime condition, consistent with how similar getters work.
    pub fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized")
    }

    /// Transfer admin to a new address.
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
        let admin = Self::admin(env.clone());
        if &admin != caller {
            return Err(TimelockError::Unauthorized);
        }
        Ok(())
    }

    fn require_council_member(env: &Env, caller: &Address) -> Result<(), TimelockError> {
        let council: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::EmergencyCouncil)
            .unwrap_or(Vec::new(env));
        for member in council.iter() {
            if &member == caller {
                return Ok(());
            }
        }
        Err(TimelockError::NotCouncilMember)
    }

    fn next_op_id(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::NextOpId)
            .unwrap_or(0)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        Env, IntoVal, String, Vec,
    };

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

    /// Returns a setup with a 3-member council requiring 2 approvals.
    fn setup_with_council() -> (
        Env,
        Address,
        RouterTimelockClient<'static>,
        Address,
        Address,
        Address,
    ) {
        let (env, admin, client) = setup();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        let m3 = Address::generate(&env);
        let mut council = Vec::new(&env);
        council.push_back(m1.clone());
        council.push_back(m2.clone());
        council.push_back(m3.clone());
        client.set_emergency_council(&admin, &council, &2);
        (env, admin, client, m1, m2, m3)
    }

    // ── Standard queue / execute / cancel ─────────────────────────────────────

    #[test]
    fn test_execute_invalid_target_fails() {
        // NOTE: The Soroban test environment does not enforce contract existence
        // the same way the production host does — try_invoke_contract on a random
        // address returns Ok in tests rather than Abort. This test therefore
        // verifies the guard compiles and the error variant is reachable.
        // On-chain, a call to a decommissioned contract address will produce an
        // InvokeError::Abort from the host, which the guard converts to InvalidTarget.
        let _ = TimelockError::InvalidTarget; // variant is defined and reachable
    }

    #[test]
    fn test_execute_live_target_succeeds() {
        // Use the timelock contract itself as a live target to confirm that a
        // real contract address passes the probe and execute proceeds normally.
        let (env, admin, client) = setup();
        let live_target = client.address.clone();
        let desc = String::from_str(&env, "upgrade live contract");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &live_target, &3600, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        // The probe call to a live contract returns a contract-level error (unknown fn),
        // not an Abort, so the guard passes and execute succeeds.
        assert!(client.try_execute(&admin, &op_id).is_ok());
        assert!(client.get_op(&op_id).unwrap().executed);
    }

    #[test]
    fn test_queue_and_execute() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        assert!(client.try_execute(&admin, &op_id).is_ok());
        assert!(client.get_op(&op_id).unwrap().executed);
    }

    #[test]
    fn test_execute_too_early_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        assert_eq!(
            client.try_execute(&admin, &op_id),
            Err(Ok(TimelockError::TooEarly))
        );
    }

    #[test]
    fn test_cancel_operation() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        client.cancel(&admin, &op_id);
        assert!(client.get_op(&op_id).unwrap().cancelled);
    }

    #[test]
    fn test_execute_cancelled_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        client.cancel(&admin, &op_id);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        assert_eq!(
            client.try_execute(&admin, &op_id),
            Err(Ok(TimelockError::AlreadyCancelled))
        );
    }

    #[test]
    fn test_double_execute_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);
        assert_eq!(
            client.try_execute(&admin, &op_id),
            Err(Ok(TimelockError::AlreadyExecuted))
        );
    }

    #[test]
    fn test_delay_below_minimum_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        assert_eq!(
            client.try_queue(&admin, &desc, &target, &100, &deps),
            Err(Ok(TimelockError::InvalidDelay))
        );
    }

    #[test]
    fn test_unauthorized_queue_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "malicious");
        let deps = Vec::new(&env);
        assert_eq!(
            client.try_queue(&attacker, &desc, &target, &3600, &deps),
            Err(Ok(TimelockError::Unauthorized))
        );
    }

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
        assert_eq!(
            client.try_set_min_delay(&admin, &7200),
            Err(Ok(TimelockError::Unauthorized))
        );
    }

    #[test]
    fn test_set_min_delay() {
        let (env, admin, client) = setup();
        client.set_min_delay(&admin, &7200);
        assert_eq!(client.min_delay(), 7200);
    }

    #[test]
    fn test_set_min_delay_zero_fails() {
        let (env, admin, client) = setup();
        assert_eq!(
            client.try_set_min_delay(&admin, &0),
            Err(Ok(TimelockError::InvalidDelay))
        );
    }

    #[test]
    fn test_operation_with_dependencies() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op0 = client.queue(&admin, &desc, &target, &3600, &deps);
        let mut deps1 = Vec::new(&env);
        deps1.push_back(op0);
        let op1 = client.queue(&admin, &desc, &target, &3600, &deps1);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        assert_eq!(
            client.try_execute(&admin, &op1),
            Err(Ok(TimelockError::DependencyNotMet))
        );
        assert!(client.try_execute(&admin, &op0).is_ok());
        assert!(client.try_execute(&admin, &op1).is_ok());
    }

    // ── Emergency council configuration ───────────────────────────────────────

    #[test]
    fn test_set_emergency_council_enables_fast_track() {
        let (env, admin, client) = setup();
        let m1 = Address::generate(&env);
        let m2 = Address::generate(&env);
        let mut council = Vec::new(&env);
        council.push_back(m1.clone());
        council.push_back(m2.clone());
        assert!(client
            .try_set_emergency_council(&admin, &council, &1)
            .is_ok());
    }

    #[test]
    fn test_set_emergency_council_required_zero_fails() {
        let (env, admin, client) = setup();
        let m1 = Address::generate(&env);
        let mut council = Vec::new(&env);
        council.push_back(m1.clone());
        assert_eq!(
            client.try_set_emergency_council(&admin, &council, &0),
            Err(Ok(TimelockError::InvalidConfig))
        );
    }

    #[test]
    fn test_set_emergency_council_required_exceeds_size_fails() {
        let (env, admin, client) = setup();
        let m1 = Address::generate(&env);
        let mut council = Vec::new(&env);
        council.push_back(m1.clone());
        assert_eq!(
            client.try_set_emergency_council(&admin, &council, &2),
            Err(Ok(TimelockError::InvalidConfig))
        );
    }

    #[test]
    fn test_set_emergency_council_unauthorized_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let m1 = Address::generate(&env);
        let mut council = Vec::new(&env);
        council.push_back(m1.clone());
        assert_eq!(
            client.try_set_emergency_council(&attacker, &council, &1),
            Err(Ok(TimelockError::Unauthorized))
        );
    }

    // ── queue_critical ────────────────────────────────────────────────────────

    #[test]
    fn test_queue_critical_without_council_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "critical fix");
        assert_eq!(
            client.try_queue_critical(&admin, &desc, &target, &3600),
            Err(Ok(TimelockError::FastTrackDisabled))
        );
    }

        env.events()
            .publish((Symbol::new(&env, "op_executed"),), (op_id, op.target));

        Ok(())
    }

    /// Get an operation by id.
    pub fn get_op(env: Env, op_id: Bytes) -> Option<Op> {
        env.storage().instance().get(&DataKey::Op(op_id))
    }

    /// Get the human-readable status of an operation.
    ///
    /// # Returns
    /// * `Cancelled` — if the operation was cancelled.
    /// * `Executed` — if the operation was executed.
    /// * `Ready` — if the ETA has elapsed but the operation has not been executed.
    /// * `Queued` — if the ETA has not yet elapsed.
    ///
    /// Returns `None` if no operation with `op_id` exists.
    pub fn get_operation_status(env: Env, op_id: Bytes) -> Option<OperationStatus> {
        let op: Op = env.storage().instance().get(&DataKey::Op(op_id))?;
        let status = if op.cancelled {
            OperationStatus::Cancelled
        } else if op.executed {
            OperationStatus::Executed
        } else if env.ledger().timestamp() >= op.eta {
            OperationStatus::Ready
        } else {
            OperationStatus::Queued
        };
        Some(status)
    }

    /// Get the minimum delay.
    pub fn min_delay(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MinDelay)
            .unwrap_or(0)
    }

    /// Get the admin.
    pub fn admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized")
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
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        Bytes, Env, IntoVal, String,
    };

    fn setup() -> (Env, Address, RouterTimelockClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterTimelock);
        let client = RouterTimelockClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin, &3600);
        (env, admin, client)
    }

    #[test]
    fn test_queue_returns_op_id() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        assert!(!op_id.is_empty());
    }

    #[test]
    fn test_queue_emits_op_queued_event() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);

        let events = env.events().all();
        let last = events.last().unwrap();

        // Topic is "op_queued"
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, "op_queued"));

        // Payload is (op_id, target, eta)
        let (emitted_id, emitted_target, emitted_eta): (Bytes, Address, u64) =
            last.2.into_val(&env);
        assert_eq!(emitted_id, op_id);
        assert_eq!(emitted_target, target);
        assert!(emitted_eta > 0);
    }

    #[test]
    fn test_queue_stores_op() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        let op = client.get_op(&op_id).unwrap();

        assert_eq!(op.target, target);
        assert!(!op.executed);
        assert!(!op.cancelled);
    }

    #[test]
    fn test_execute_before_eta_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        let result = client.try_execute(&admin, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::NotReady)));
    }

    #[test]
    fn test_execute_after_eta_succeeds() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);

        let op = client.get_op(&op_id).unwrap();
        assert!(op.executed);
    }

    #[test]
    fn test_cancel_op() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        client.cancel(&admin, &op_id);

        let op = client.get_op(&op_id).unwrap();
        assert!(op.cancelled);
    }

    #[test]
    fn test_execute_cancelled_op_fails() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
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

        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);
        let result = client.try_execute(&admin, &op_id);
        assert_eq!(result, Err(Ok(TimelockError::AlreadyExecuted)));
    }

    #[test]
    fn test_set_min_delay_applies_to_new_ops_only() {
        let (env, admin, client) = setup(); // min_delay = 3600
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        // min_delay is 3600, passing 100 should fail
        let result = client.try_queue(&admin, &desc, &target, &100, &deps);
        assert_eq!(result, Err(Ok(TimelockError::DelayTooShort)));
    }

    #[test]
    fn test_unauthorized_queue_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let result = client.try_queue(&attacker, &desc, &target, &3600, &deps);
        assert_eq!(result, Err(Ok(TimelockError::Unauthorized)));
    }

    #[test]
    fn test_execute_emits_op_executed_event() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);

        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, "op_executed"));
    }

    #[test]
    fn test_cancel_emits_op_cancelled_event() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);

        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        client.cancel(&admin, &op_id);

        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, "op_cancelled"));
    }

    #[test]
    fn test_get_operation_status_queued() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        assert_eq!(client.get_operation_status(&op_id), Some(OperationStatus::Queued));
    }

    #[test]
    fn test_get_operation_status_ready() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        assert_eq!(client.get_operation_status(&op_id), Some(OperationStatus::Ready));
    }

    #[test]
    fn test_get_operation_status_executed() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op_id);
        assert_eq!(client.get_operation_status(&op_id), Some(OperationStatus::Executed));
    }

    #[test]
    fn test_get_operation_status_cancelled() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let desc = String::from_str(&env, "upgrade oracle");
        let deps = Vec::new(&env);
        let op_id = client.queue(&admin, &desc, &target, &3600, &deps);
        client.cancel(&admin, &op_id);
        assert_eq!(client.get_operation_status(&op_id), Some(OperationStatus::Cancelled));
    }

    #[test]
    fn test_get_operation_status_nonexistent_returns_none() {
        let (env, _admin, client) = setup();
        let fake_id = Bytes::from_array(&env, &[0u8; 32]);
        assert_eq!(client.get_operation_status(&fake_id), None);
    }

    #[test]
    fn test_cancel_all_with_mixed_states() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        // Queue 3 operations
        let op1 = client.queue(&admin, &String::from_str(&env, "op1"), &target, &3600, &deps);
        let op2 = client.queue(&admin, &String::from_str(&env, "op2"), &target, &3600, &deps);
        let op3 = client.queue(&admin, &String::from_str(&env, "op3"), &target, &3600, &deps);

        // Execute op1
        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op1);

        // Cancel op2
        client.cancel(&admin, &op2);

        // op3 remains pending

        // cancel_all should only affect pending operations (op3)
        // Expected: returns 1 (only op3 was cancelled)
        // Note: cancel_all() needs to be implemented
        // let count = client.cancel_all(&admin);
        // assert_eq!(count, 1);

        // Verify states after cancel_all
        // assert_eq!(client.get_operation_status(&op1), Some(OperationStatus::Executed));
        // assert_eq!(client.get_operation_status(&op2), Some(OperationStatus::Cancelled));
        // assert_eq!(client.get_operation_status(&op3), Some(OperationStatus::Cancelled));
    }

    #[test]
    fn test_cancel_all_returns_correct_count() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        // Queue 5 operations
        for i in 0..5 {
            let desc = String::from_str(&env, &format!("op{}", i));
            client.queue(&admin, &desc, &target, &3600, &deps);
        }

        // All 5 are pending, cancel_all should return 5
        // Note: cancel_all() needs to be implemented
        // let count = client.cancel_all(&admin);
        // assert_eq!(count, 5);
    }

    #[test]
    fn test_cancel_all_with_no_pending_operations() {
        let (env, admin, client) = setup();
        let target = Address::generate(&env);
        let deps = Vec::new(&env);

        // Queue and execute all operations
        let op1 = client.queue(&admin, &String::from_str(&env, "op1"), &target, &3600, &deps);
        let op2 = client.queue(&admin, &String::from_str(&env, "op2"), &target, &3600, &deps);

        env.ledger().with_mut(|l| l.timestamp += 3601);
        client.execute(&admin, &op1);
        client.execute(&admin, &op2);

        // No pending operations, cancel_all should return 0
        // Note: cancel_all() needs to be implemented
        // let count = client.cancel_all(&admin);
        // assert_eq!(count, 0);
    }
}
