#![no_std]

//! # router-access
//!
//! Role-based access control for the stellar-router suite.

use router_common;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, String, Symbol, Vec,
};

const MAX_HIERARCHY_DEPTH: u32 = 16;

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    SuperAdmin,
    HasRole(String, Address), // (role, address) -> bool
    RoleAdmin(String),        // role -> Address who manages it
    Blacklisted(Address),

    RoleMembers(String),     // role -> Vec<Address>
    RoleMemberCount(String), // role -> u32 (active members)

    AddressRoles(Address), // address -> Vec<String>
    RoleExpiry(String, Address),
    RoleParent(String), // child role -> parent role
    AllRoles, // Vec<String> — all roles ever defined in the system
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AccessError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    AlreadyHasRole = 4,
    RoleNotFound = 5,
    Blacklisted = 6,
    CannotBlacklistAdmin = 7,
    DestinationAlreadyHasRole = 8,
    HierarchyCycle = 8,
}


// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RouterAccess;

#[contractimpl]
impl RouterAccess {
    /// Initialize with a super-admin.
    pub fn initialize(env: Env, super_admin: Address) -> Result<(), AccessError> {
        if env.storage().instance().has(&DataKey::SuperAdmin) {
            return Err(AccessError::AlreadyInitialized);
        }
        env.storage()
            .instance()
            .set(&DataKey::SuperAdmin, &super_admin);
        Ok(())
    }

    /// Grant a role to an address.
    pub fn grant_role(
        env: Env,
        admin: Address,
        account: Address,
        role: String,
        expires_in: Option<u64>,
    ) -> Result<(), AccessError> {
        admin.require_auth();
        Self::require_role_manager(&env, &admin, &role)?;
        Self::grant_role_internal(&env, &account, &role, expires_in)
    }

    /// Grant a role to multiple accounts in one call.
    pub fn grant_role_batch(
        env: Env,
        admin: Address,
        accounts: Vec<Address>,
        role: String,
        expires_in: Option<u64>,
        fail_fast: bool,
    ) -> Result<router_common::BatchResult, AccessError> {
        admin.require_auth();
        Self::require_role_manager(&env, &admin, &role)?;
        let mut result = router_common::BatchResult::new(&env);
        for (index, account) in accounts.iter().enumerate() {
            let idx = index as u32;
            match Self::grant_role_internal(&env, &account, &role, expires_in) {
                Ok(()) => result.record_success(idx),
                Err(err) => {
                    result.record_failure(idx, Self::access_error_to_batch(&env, err));
                    if fail_fast {
                        break;
                    }
                }
            }
        }
        Ok(result)
    }

    /// Removes `role` from `target`.
    pub fn revoke_role(
        env: Env,
        caller: Address,
        role: String,
        target: Address,
    ) -> Result<(), AccessError> {
        caller.require_auth();
        Self::require_role_manager(&env, &caller, &role)?;

        // Check the raw storage key — not has_role_internal — so that expired
        // roles (where has_role_internal returns false) can still be revoked
        // to clean up storage.
        let key = DataKey::HasRole(role.clone(), target.clone());
        if !env.storage().instance().has(&key) {
            return Err(AccessError::RoleNotFound);
        }

        // Decrement active-member counter only if this grant was currently active.
        // (If the role is expired, it may still exist in HasRole but shouldn't be
        // counted as an active member.)
        let was_active = Self::has_role_internal(&env, &target, &role);

        env.storage().instance().remove(&key);

        if was_active {
            let current: u32 = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::RoleMemberCount(role.clone()))
                .unwrap_or(0u32);
            let new_count = current.saturating_sub(1);
            env.storage()
                .instance()
                .set(&DataKey::RoleMemberCount(role.clone()), &new_count);
        }

        let mut members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::RoleMembers(role.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        if let Some(i) = members.iter().position(|a| a == target) {
            members.remove(i as u32);
        }
        env.storage()
            .instance()
            .set(&DataKey::RoleMembers(role.clone()), &members);

        let mut roles: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::AddressRoles(target.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        if let Some(i) = roles.iter().position(|r| r == role) {
            roles.remove(i as u32);
        }
        env.storage()
            .instance()
            .set(&DataKey::AddressRoles(target.clone()), &roles);

        env.storage()
            .instance()
            .remove(&DataKey::RoleExpiry(role.clone(), target.clone()));

        // Keep RoleMemberCount consistent for expiry-based removal.
        if Self::has_role_internal(&env, &target, &role) {
            let current: u32 = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::RoleMemberCount(role.clone()))
                .unwrap_or(0u32);
            let new_count = current.saturating_sub(1);
            env.storage()
                .instance()
                .set(&DataKey::RoleMemberCount(role.clone()), &new_count);
        }

        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_ROLE_REVOKED),),
            (role, target),
        );
        Ok(())
    }

    /// Check if an address has a role (and it has not expired).
    pub fn has_role(env: Env, account: Address, role: String) -> bool {
        Self::has_role_internal(&env, &account, &role)
    }

    /// Check if a role has expired for an address.
    pub fn is_role_expired(env: Env, role: String, target: Address) -> bool {
        // View helper: counter is maintained for active members, but expiry still
        // uses RoleExpiry storage.

        if let Some(expires_at) = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::RoleExpiry(role, target))
        {
            let current_timestamp = env.ledger().timestamp();
            current_timestamp >= expires_at
        } else {
            false
        }
    }

    /// Return the expiry timestamp for a role grant, or None if no expiry is set.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `role` - The role name.
    /// * `target` - The address whose expiry to query.
    ///
    /// # Returns
    /// `Some(timestamp)` if an expiry exists, `None` otherwise.
    pub fn get_role_member_count(env: Env, role: String) -> u32 {
        env.storage()
            .instance()
            .get::<DataKey, u32>(&DataKey::RoleMemberCount(role))
            .unwrap_or(0u32)
    }

    pub fn get_role_expiry(env: Env, role: String, target: Address) -> Option<u64> {
        env.storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::RoleExpiry(role, target))
    }

    /// Set the admin for a specific role.
    pub fn set_role_admin(
        env: Env,
        caller: Address,
        role: String,
        admin: Address,
    ) -> Result<(), AccessError> {
        caller.require_auth();
        Self::require_super_admin(&env, &caller)?;
        if Self::is_blacklisted_internal(&env, &admin) {
            return Err(AccessError::Blacklisted);
        }
        // Track this role in AllRoles if it's the first time we've seen it
        Self::track_role_in_all_roles(&env, &role);
        env.storage()
            .instance()
            .set(&DataKey::RoleAdmin(role.clone()), &admin);
        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_ROLE_ADMIN_SET),),
            (role, admin),
        );
        Ok(())
    }

    /// Returns the role admin for the given role, or None if none is set.
    pub fn get_role_admin(env: Env, role: String) -> Option<Address> {
        env.storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::RoleAdmin(role))
    }

    /// Set a parent role for inheritance.
    pub fn set_role_parent(
        env: Env,
        caller: Address,
        role: String,
        parent_role: String,
    ) -> Result<(), AccessError> {
        caller.require_auth();
        Self::require_super_admin(&env, &caller)?;
        Self::ensure_no_role_parent_cycle(&env, &role, &parent_role)?;

        Self::track_role_in_all_roles(&env, &role);
        Self::track_role_in_all_roles(&env, &parent_role);

        env.storage()
            .instance()
            .set(&DataKey::RoleParent(role.clone()), &parent_role);
        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_ROLE_PARENT_SET),),
            (role, parent_role),
        );
        Ok(())
    }

    /// Returns the parent role for the given role, or None if none is set.
    pub fn get_role_parent(env: Env, role: String) -> Option<String> {
        env.storage()
            .instance()
            .get::<DataKey, String>(&DataKey::RoleParent(role))
    }

    /// List all roles that have ever been defined in the system.
    ///
    /// This is the roles equivalent of `router-core`'s `get_all_routes()`.
    /// Returns all role names that have been tracked via `grant_role()` or
    /// `set_role_admin()`. Roles are never removed from this list even if all
    /// members are revoked — this preserves an audit trail of all roles that
    /// have existed.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// A [`Vec<String>`] of all role names in the system.
    pub fn list_all_roles(env: Env) -> Vec<String> {
        env.storage()
            .instance()
            .get(&DataKey::AllRoles)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Blacklist an address.
    pub fn blacklist(env: Env, caller: Address, target: Address) -> Result<(), AccessError> {
        caller.require_auth();
        Self::require_super_admin(&env, &caller)?;

        let super_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::SuperAdmin)
            .ok_or(AccessError::NotInitialized)?;
        if target == super_admin {
            return Err(AccessError::CannotBlacklistAdmin);
        }

        env.storage()
            .instance()
            .set(&DataKey::Blacklisted(target.clone()), &true);
        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_ADDRESS_BLACKLISTED),),
            target,
        );
        Ok(())
    }

    /// Remove from blacklist.
    pub fn unblacklist(env: Env, caller: Address, target: Address) -> Result<(), AccessError> {
        caller.require_auth();
        Self::require_super_admin(&env, &caller)?;
        env.storage()
            .instance()
            .remove(&DataKey::Blacklisted(target.clone()));
        env.events().publish(
            (Symbol::new(
                &env,
                router_common::EVENT_ADDRESS_UNBLACKLISTED,
            ),),
            target,
        );
        Ok(())
    }

    pub fn is_blacklisted(env: Env, target: Address) -> bool {
        Self::is_blacklisted_internal(&env, &target)
    }

    fn is_blacklisted_internal(env: &Env, target: &Address) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Blacklisted(target.clone()))
            .unwrap_or(false)
    }

    pub fn get_role_members(env: Env, role: String) -> Vec<Address> {
        let all_members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::RoleMembers(role.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        // Filter out expired roles
        let mut active_members = Vec::new(&env);
        for member in all_members.iter() {
            if Self::has_role_internal(&env, &member, &role) {
                active_members.push_back(member.clone());
            }
        }
        active_members
    }

    pub fn get_roles_for_address(env: Env, addr: Address) -> Vec<String> {
        env.storage()
            .instance()
            .get(&DataKey::AddressRoles(addr))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Transfer an existing role grant from one address to another.
    ///
    /// Semantics (per task): transfers only if `from` currently has the role
    /// *active* (i.e. not expired). Expired grants on `from` are rejected.
    ///
    /// The destination `to` receives the same expiry timestamp as `from`.
    pub fn transfer_role_membership(
        env: Env,
        caller: Address,
        role: String,
        from: Address,
        to: Address,
    ) -> Result<(), AccessError> {
        caller.require_auth();
        Self::require_role_manager(&env, &caller, &role)?;

        if from == to {
        // No-op but still validate that `from` currently has the role active.
        if !Self::has_role_internal(&env, &from, &role) {
            return Err(AccessError::RoleNotFound);
        }
        if from == to {
            return Ok(());
        }

        // Must be active on `from`.
        if !Self::has_role_internal(&env, &from, &role) {
            return Err(AccessError::RoleNotFound);
        }


        // Do not overwrite an existing active assignment on destination.
        if Self::has_role_internal(&env, &to, &role) {
            return Err(AccessError::DestinationAlreadyHasRole);
        }

        // Read expiry timestamp from storage. Since from is active, this should exist.
        let expiry = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::RoleExpiry(role.clone(), from.clone()))
            .ok_or(AccessError::RoleNotFound)?;

        // Remove grant from source (including member counters/lists).
        let from_key = DataKey::HasRole(role.clone(), from.clone());
        if !env.storage().instance().has(&from_key) {
            return Err(AccessError::RoleNotFound);
        }

        let was_active = true; // we already validated from is active
        env.storage().instance().remove(&from_key);
        if was_active {
            let current: u32 = env
                .storage()
                .instance()
                .get::<DataKey, u32>(&DataKey::RoleMemberCount(role.clone()))
                .unwrap_or(0u32);
            let new_count = current.saturating_sub(1);
            env.storage().instance().set(&DataKey::RoleMemberCount(role.clone()), &new_count);
        }

        let mut members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::RoleMembers(role.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        if let Some(i) = members.iter().position(|a| a == from) {
            members.remove(i as u32);
        }
        env.storage()
            .instance()
            .set(&DataKey::RoleMembers(role.clone()), &members);

        let mut roles: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::AddressRoles(from.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        if let Some(i) = roles.iter().position(|r| r == &role) {
            roles.remove(i as u32);
        }
        env.storage()
            .instance()
            .set(&DataKey::AddressRoles(from.clone()), &roles);

        env.storage()
            .instance()
            .remove(&DataKey::RoleExpiry(role.clone(), from.clone()));

        // Grant to destination with same expiry timestamp.
        if Self::is_blacklisted_internal(&env, &to) {
            return Err(AccessError::Blacklisted);
        }

        // Track this role in AllRoles if it's the first time we've seen it.
        Self::track_role_in_all_roles(&env, &role);

        env.storage()
            .instance()
            .set(&DataKey::HasRole(role.clone(), to.clone()), &true);

        let mut members_to: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::RoleMembers(role.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        if !members_to.iter().any(|a| a == &to) {
            members_to.push_back(to.clone());
        }
        env.storage()
            .instance()
            .set(&DataKey::RoleMembers(role.clone()), &members_to);

        let mut roles_to: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::AddressRoles(to.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        if !roles_to.iter().any(|r| r == &role) {
            roles_to.push_back(role.clone());
        }
        env.storage()
            .instance()
            .set(&DataKey::AddressRoles(to.clone()), &roles_to);

        let key_to = DataKey::RoleExpiry(role.clone(), to.clone());
        env.storage().instance().set(&key_to, &expiry);

        // Since from was active and we preserve expiry, destination should become active too.
        let current: u32 = env
            .storage()
            .instance()
            .get::<DataKey, u32>(&DataKey::RoleMemberCount(role.clone()))
            .unwrap_or(0u32);
        let new_count = current.saturating_add(1);
        env.storage()
            .instance()
            .set(&DataKey::RoleMemberCount(role.clone()), &new_count);

        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_ROLE_GRANTED),),
            (to.clone(), role.clone(), expiry),
        );

        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_ROLE_REVOKED),),
            (role, from),
        );

        Ok(())
    }

    pub fn transfer_super_admin(
        env: Env,
        current: Address,
        new_admin: Address,
    ) -> Result<(), AccessError> {

        current.require_auth();
        Self::require_super_admin(&env, &current)?;
        env.storage()
            .instance()
            .set(&DataKey::SuperAdmin, &new_admin);
        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_ADMIN_TRANSFERRED),),
            (current, new_admin),
        );
        Ok(())
    }

    pub fn super_admin(env: Env) -> Result<Address, AccessError> {
        env.storage()
            .instance()
            .get(&DataKey::SuperAdmin)
            .ok_or(AccessError::NotInitialized)
    }

    pub fn expire_role(
        env: Env,
        caller: Address,
        role: String,
        target: Address,
    ) -> Result<(), AccessError> {
        caller.require_auth();
        Self::require_super_admin(&env, &caller)?;
        env.storage()
            .instance()
            .remove(&DataKey::RoleExpiry(role.clone(), target.clone()));
        env.storage()
            .instance()
            .remove(&DataKey::HasRole(role.clone(), target.clone()));
        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_ROLE_EXPIRED),),
            (role, target),
        );
        Ok(())
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Track a role name in the AllRoles list if it hasn't been seen before.
    fn track_role_in_all_roles(env: &Env, role: &String) {
        let mut all_roles: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::AllRoles)
            .unwrap_or_else(|| Vec::new(env));
        if !all_roles.iter().any(|r| r == *role) {
            all_roles.push_back(role.clone());
            env.storage().instance().set(&DataKey::AllRoles, &all_roles);
        }
    }

    fn access_error_to_batch(env: &Env, err: AccessError) -> router_common::BatchItemError {
        match err {
            AccessError::AlreadyHasRole => router_common::BatchItemError::AlreadyExists,
            AccessError::Unauthorized => router_common::BatchItemError::Unauthorized,
            AccessError::Blacklisted => router_common::BatchItemError::InvalidMetadata,
            _ => router_common::BatchItemError::Custom(
                soroban_sdk::String::from_str(env, "Error"),
            ),
        }
    }

    fn ensure_no_role_parent_cycle(
        env: &Env,
        role: &String,
        parent_role: &String,
    ) -> Result<(), AccessError> {
        let mut current = parent_role.clone();

        loop {
            if &current == role {
                return Err(AccessError::HierarchyCycle);
            }

            match env
                .storage()
                .instance()
                .get::<DataKey, String>(&DataKey::RoleParent(current.clone()))
            {
                Some(parent) => current = parent,
                None => return Ok(()),
            }
        }
    }

    fn grant_role_internal(
        env: &Env,
        account: &Address,
        role: &String,
        expires_in: Option<u64>,
    ) -> Result<(), AccessError> {
        // Grant can transition an (role, account) pair from inactive to active.
        // Maintain RoleMemberCount without iterating RoleMembers.

        if Self::is_blacklisted_internal(env, account) {
            return Err(AccessError::Blacklisted);
        }

        let raw_key = DataKey::HasRole(role.clone(), account.clone());
        let has_raw_assignment = env.storage().instance().has(&raw_key);

        // If there is an existing unexpired assignment, only treat it as a duplicate error when
        // the requested expiry matches the existing expiry.
        //
        // This allows admins to extend/shorten expiry (or remove it by granting with `None`).
        if has_raw_assignment && Self::has_role_internal(env, account, role) {
            let existing_expiry: Option<u64> = env
                .storage()
                .instance()
                .get::<DataKey, u64>(&DataKey::RoleExpiry(role.clone(), account.clone()));

            let requested_expiry = match expires_in {
                Some(seconds) => env.ledger().timestamp() + seconds,
                None => u64::MAX,
            };

            let existing_expiry = existing_expiry.unwrap_or(u64::MAX);
            if existing_expiry == requested_expiry {
                return Err(AccessError::AlreadyHasRole);
            }
        }

        // Track this role in AllRoles if it's the first time we've seen it
        Self::track_role_in_all_roles(env, role);

        let expiry_timestamp = match expires_in {
            Some(seconds) => env.ledger().timestamp() + seconds,
            None => u64::MAX,
        };

        env.storage()
            .instance()
            .set(&DataKey::HasRole(role.clone(), account.clone()), &true);

        let mut members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::RoleMembers(role.clone()))
            .unwrap_or_else(|| Vec::new(env));
        if !members.iter().any(|a| a == *account) {
            members.push_back(account.clone());
        }
        env.storage()
            .instance()
            .set(&DataKey::RoleMembers(role.clone()), &members);

        let mut roles: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::AddressRoles(account.clone()))
            .unwrap_or_else(|| Vec::new(env));
        if !roles.iter().any(|r| r == *role) {
            roles.push_back(role.clone());
        }
        env.storage()
            .instance()
            .set(&DataKey::AddressRoles(account.clone()), &roles);

        let key = DataKey::RoleExpiry(role.clone(), account.clone());
        env.storage().instance().set(&key, &expiry_timestamp);

        env.events().publish(
            (Symbol::new(env, router_common::EVENT_ROLE_GRANT),),
            (account.clone(), role.clone(), expiry_timestamp),
        );
        Ok(())
    }

    fn require_super_admin(env: &Env, caller: &Address) -> Result<(), AccessError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::SuperAdmin)
            .ok_or(AccessError::NotInitialized)?;
        if &admin != caller {
            return Err(AccessError::Unauthorized);
        }
        Ok(())
    }

    fn require_role_manager(env: &Env, caller: &Address, role: &String) -> Result<(), AccessError> {
        if Self::is_blacklisted_internal(env, caller) {
            return Err(AccessError::Blacklisted);
        }
        if let Some(admin) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::SuperAdmin)
        {
            if &admin == caller {
                return Ok(());
            }
        }
        if let Some(role_admin) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::RoleAdmin(role.clone()))
        {
            if &role_admin == caller {
                return Ok(());
            }
        }
        Err(AccessError::Unauthorized)
    }

    fn has_role_internal(env: &Env, account: &Address, role: &String) -> bool {

        if Self::is_blacklisted_internal(env, account) {
            return false;
        }

        let mut current_role = role.clone();
        let mut depth = 0u32;

        loop {
            if Self::has_direct_role_internal(env, account, &current_role) {
                return true;
            }

            depth += 1;
            if depth >= MAX_HIERARCHY_DEPTH {
                return false;
            }

            match env
                .storage()
                .instance()
                .get::<DataKey, String>(&DataKey::RoleParent(current_role.clone()))
            {
                Some(parent_role) => current_role = parent_role,
                None => return false,
            }
        }
    }

    fn has_direct_role_internal(env: &Env, account: &Address, role: &String) -> bool {
        let has_role = env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::HasRole(role.clone(), account.clone()))
            .unwrap_or(false);

        if !has_role {
            return false;
        }

        // Check if role has expired
        if let Some(expires_at) = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::RoleExpiry(role.clone(), account.clone()))
        {
            let current_timestamp = env.ledger().timestamp();
            if current_timestamp >= expires_at {
                return false;
            }
        }

        true
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        vec, Env, IntoVal, Symbol,
    };

    fn setup() -> (Env, Address, RouterAccessClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterAccess);
        let client = RouterAccessClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    // ... (all your existing tests remain unchanged) ...

    #[test]
    fn test_expired_role_not_recognized() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        client.grant_role(&admin, &user, &role, &Some(10));

        env.ledger().set_timestamp(env.ledger().timestamp() + 20);

        assert!(!client.has_role(&user, &role));
    }

    #[test]
    fn test_role_expires_correctly_with_timestamp() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        client.grant_role(&admin, &user, &role, &Some(1));

        env.ledger().set_timestamp(env.ledger().timestamp() + 5);

        assert!(!client.has_role(&user, &role));
    }

    #[test]
    fn test_set_role_admin_emits_event() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let new_role_admin = Address::generate(&env);

        client.set_role_admin(&admin, &role, &new_role_admin);

        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, "role_admin_set"));
        let (emitted_role, emitted_admin): (String, Address) = last.2.into_val(&env);
        assert_eq!(emitted_role, role);
        assert_eq!(emitted_admin, new_role_admin);
    }

    #[test]
    fn test_set_role_admin_rejects_blacklisted_address() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let blacklisted_addr = Address::generate(&env);

        // Blacklist the address
        client.blacklist(&admin, &blacklisted_addr);

        // Try to set blacklisted address as role admin
        let result = client.try_set_role_admin(&admin, &role, &blacklisted_addr);
        assert_eq!(result, Err(Ok(AccessError::Blacklisted)));
    }

    #[test]
    fn test_set_role_admin_valid_address_succeeds() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let valid_addr = Address::generate(&env);

        // Set a non-blacklisted address as role admin
        client.set_role_admin(&admin, &role, &valid_addr);

        // Verify the role admin was set correctly
        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, "role_admin_set"));
        let (emitted_role, emitted_admin): (String, Address) = last.2.into_val(&env);
        assert_eq!(emitted_role, role);
        assert_eq!(emitted_admin, valid_addr);
    }

    #[test]
    fn test_blacklisted_role_admin_cannot_grant() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "editor");
        let attacker = Address::generate(&env);
        let victim = Address::generate(&env);

        // Designate attacker as editor admin
        client.set_role_admin(&admin, &role, &attacker);

        // Blacklist the attacker
        client.blacklist(&admin, &attacker);

        // Try to grant role - should fail with Blacklisted
        let result = client.try_grant_role(&attacker, &victim, &role, &None);
        assert_eq!(result, Err(Ok(AccessError::Blacklisted)));
    }

    #[test]
    fn test_blacklisted_role_admin_cannot_revoke() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "editor");
        let attacker = Address::generate(&env);
        let victim = Address::generate(&env);

        // Designate attacker as editor admin
        client.set_role_admin(&admin, &role, &attacker);

        // Grant role to victim
        client.grant_role(&admin, &victim, &role, &None);

        // Blacklist the attacker
        client.blacklist(&admin, &attacker);

        // Try to revoke role - should fail with Blacklisted
        let result = client.try_revoke_role(&attacker, &role, &victim);
        assert_eq!(result, Err(Ok(AccessError::Blacklisted)));
    }

    // ── Issue #174: grant_role missing writes ────────────────────────────────

    #[test]
    fn test_revoke_role_succeeds_after_grant() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "editor");
        let user = Address::generate(&env);

        // Grant the role
        client.grant_role(&admin, &user, &role, &None);

        // Revoke should succeed (not return RoleNotFound)
        let result = client.try_revoke_role(&admin, &role, &user);
        assert!(result.is_ok(), "revoke_role should succeed after grant");

        // Verify role is no longer present
        assert!(!client.has_role(&user, &role));
    }

    #[test]
    fn test_revoke_role_removes_expiry() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "editor");
        let user = Address::generate(&env);

        client.grant_role(&admin, &user, &role, &Some(100));

        client.revoke_role(&admin, &role, &user);

        // After revoke_role, is_role_expired returns false
        assert!(!client.is_role_expired(&role, &user));

        // No RoleExpiry key exists in storage
        let has_expiry: bool = env.as_contract(&client.address, || {
            env.storage()
                .instance()
                .has(&DataKey::RoleExpiry(role.clone(), user.clone()))
        });
        assert!(!has_expiry);
    }

    #[test]
    fn test_get_role_members_populated_after_grant() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "editor");
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);

        // Initially, role should have no members
        let members_before = client.get_role_members(&role);
        assert!(members_before.is_empty());

        // Grant role to user1
        client.grant_role(&admin, &user1, &role, &None);

        // Check that user1 is in role members
        let members_after_first = client.get_role_members(&role);
        assert_eq!(members_after_first.len(), 1);
        assert!(members_after_first.contains(&user1));

        // Grant role to user2
        client.grant_role(&admin, &user2, &role, &None);

        // Check that both users are in role members
        let members_after_second = client.get_role_members(&role);
        assert_eq!(members_after_second.len(), 2);
        assert!(members_after_second.contains(&user1));
        assert!(members_after_second.contains(&user2));
    }

    // Issue #175: grant_role missing guards

    #[test]
    fn test_grant_role_blacklisted_account_fails() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let blacklisted_user = Address::generate(&env);

        client.blacklist(&admin, &blacklisted_user);

        let result = client.try_grant_role(&admin, &blacklisted_user, &role, &None);
        assert_eq!(result, Err(Ok(AccessError::Blacklisted)));
    }

    #[test]
    fn test_grant_role_already_has_role_fails() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        // Duplicate grant with identical expiry should fail.
        client.grant_role(&admin, &user, &role, &None);

        let result = client.try_grant_role(&admin, &user, &role, &None);
        assert_eq!(result, Err(Ok(AccessError::AlreadyHasRole)));
    }

    #[test]
    fn test_grant_role_returns_error_on_unauthorized() {
        let (env, _admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let unauthorized = Address::generate(&env);
        let user = Address::generate(&env);

        let result = client.try_grant_role(&unauthorized, &user, &role, &None);
        assert_eq!(result, Err(Ok(AccessError::Unauthorized)));
    }

    #[test]
    fn test_grant_role_already_has_role_duplicate_with_identical_expiry_fails() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        client.grant_role(&admin, &user, &role, &Some(100));

        let result = client.try_grant_role(&admin, &user, &role, &Some(100));
        assert_eq!(result, Err(Ok(AccessError::AlreadyHasRole)));
    }

    #[test]
    fn test_grant_role_extends_expiry_if_role_exists() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        let now = env.ledger().timestamp();
        client.grant_role(&admin, &user, &role, &Some(100));
        assert_eq!(client.get_role_expiry(&role, &user), Some(now + 100));

        // Extend expiry
        client.grant_role(&admin, &user, &role, &Some(200));
        assert_eq!(client.get_role_expiry(&role, &user), Some(now + 200));
        assert!(client.has_role(&user, &role));
    }

    #[test]
    fn test_grant_role_shortens_expiry_if_role_exists() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        let now = env.ledger().timestamp();
        client.grant_role(&admin, &user, &role, &Some(200));
        assert_eq!(client.get_role_expiry(&role, &user), Some(now + 200));

        // Shorten expiry
        client.grant_role(&admin, &user, &role, &Some(50));
        assert_eq!(client.get_role_expiry(&role, &user), Some(now + 50));
        assert!(client.has_role(&user, &role));

        // After the shortened expiry, role should be considered expired
        env.ledger().set_timestamp(now + 51);
        assert!(!client.has_role(&user, &role));
    }

    #[test]
    fn test_grant_role_after_expiry_succeeds() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        let now = env.ledger().timestamp();
        client.grant_role(&admin, &user, &role, &Some(100));
        env.ledger().set_timestamp(now + 101);
        assert!(!client.has_role(&user, &role));

        // Re-grant should succeed after expiry.
        assert!(client
            .try_grant_role(&admin, &user, &role, &Some(100))
            .is_ok());
        assert!(client.has_role(&user, &role));
    }

    #[test]
    fn test_grant_role_none_expiry_over_existing_some_makes_permanent() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        client.grant_role(&admin, &user, &role, &Some(100));
        assert!(client.has_role(&user, &role));
        assert_ne!(client.get_role_expiry(&role, &user), Some(u64::MAX));

        // Upgrade to permanent
        client.grant_role(&admin, &user, &role, &None);
        assert_eq!(client.get_role_expiry(&role, &user), Some(u64::MAX));

        // Still active far in the future
        let future = env.ledger().timestamp() + 10_000;
        env.ledger().set_timestamp(future);
        assert!(client.has_role(&user, &role));
    }

    #[test]
    fn test_blacklisted_address_cannot_use_role() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        client.grant_role(&admin, &user, &role, &None);
        assert!(client.has_role(&user, &role));

        client.blacklist(&admin, &user);
        assert!(!client.has_role(&user, &role));

        client.unblacklist(&admin, &user);
        assert!(client.has_role(&user, &role));
    }

    #[test]
    fn test_get_roles_for_address_populated_after_grant() {
        let (env, admin, client) = setup();
        let user = Address::generate(&env);
        let role1 = String::from_str(&env, "editor");
        let role2 = String::from_str(&env, "viewer");

        // Initially, user should have no roles
        let roles_before = client.get_roles_for_address(&user);
        assert!(roles_before.is_empty());

        // Grant role1 to user
        client.grant_role(&admin, &user, &role1, &None);

        // Check that role1 is in user's roles
        let roles_after_first = client.get_roles_for_address(&user);
        assert_eq!(roles_after_first.len(), 1);
        assert!(roles_after_first.contains(&role1));

        // Grant role2 to user
        client.grant_role(&admin, &user, &role2, &None);

        // Check that both roles are in user's roles
        let roles_after_second = client.get_roles_for_address(&user);
        assert_eq!(roles_after_second.len(), 2);
        assert!(roles_after_second.contains(&role1));
        assert!(roles_after_second.contains(&role2));
    }

    #[test]
    fn test_old_super_admin_locked_out_after_transfer() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_super_admin(&admin, &new_admin);

        // Old admin should no longer be able to call super-admin functions.
        // Use the correct grant_role argument order: (admin, account, role, expires_in).
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        assert_eq!(
            client.try_grant_role(&admin, &user, &role, &None),
            Err(Ok(AccessError::Unauthorized))
        );

        // New admin should be able to grant roles.
        assert!(client
            .try_grant_role(&new_admin, &user, &role, &None)
            .is_ok());
    }

    #[test]
    fn test_transfer_super_admin_to_self_succeeds() {
        // Edge case: transferring to self should be a no-op but not error
        let (env, admin, client) = setup();
        assert!(client.try_transfer_super_admin(&admin, &admin).is_ok());
        assert_eq!(client.super_admin(), admin);
    }

    #[test]
    fn test_transfer_super_admin_unauthorized_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        assert_eq!(
            client.try_transfer_super_admin(&attacker, &attacker),
            Err(Ok(AccessError::Unauthorized))
        );
    }

    #[test]
    fn test_revoke_role_removes_storage_key() {
        // Verifies revoke_role removes the HasRole key rather than setting it to false,
        // so a subsequent grant_role on the same (role, target) pair succeeds.
        // grant_role uses signature (admin, account, role, expires_in).
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        client.grant_role(&admin, &user, &role, &None);
        assert!(client.has_role(&user, &role));
        client.revoke_role(&admin, &role, &user);
        assert!(!client.has_role(&user, &role));
        // Re-granting must succeed — if the key was set to false instead of removed,
        // has_role_internal would return false but the key would still exist,
        // and a future implementation checking .has() would wrongly block the grant.
        assert!(client.try_grant_role(&admin, &user, &role, &None).is_ok());
        assert!(client.has_role(&user, &role));
    }

    #[test]
    fn test_revoke_nonexistent_role_fails() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        // Never granted — should return RoleNotFound
        let result = client.try_revoke_role(&admin, &role, &user);
        assert_eq!(result, Err(Ok(AccessError::RoleNotFound)));
    }

    #[test]
    fn test_expire_role_removes_access() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        // Grant with a long expiry
        client.grant_role(&admin, &user, &role, &Some(9999));
        assert!(client.has_role(&user, &role));
        // Force-expire the role
        client.expire_role(&admin, &role, &user);
        assert!(!client.has_role(&user, &role));
    }

    #[test]
    fn test_expire_role_allows_regrant() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        client.grant_role(&admin, &user, &role, &Some(9999));
        client.expire_role(&admin, &role, &user);
        // Should be able to grant again
        assert!(client
            .try_grant_role(&admin, &user, &role, &Some(9999))
            .is_ok());
        assert!(client.has_role(&user, &role));
    }

    #[test]
    fn test_expire_role_unauthorized_fails() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        let attacker = Address::generate(&env);
        client.grant_role(&admin, &user, &role, &Some(9999));
        let result = client.try_expire_role(&attacker, &role, &user);
        assert_eq!(result, Err(Ok(AccessError::Unauthorized)));
    }

    #[test]
    fn test_revoke_role_emits_event() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        client.grant_role(&admin, &user, &role, &None);
        client.revoke_role(&admin, &role, &user);

        let events = env.events().all();
        let last = events.last().unwrap();
        let topic: Symbol = last.1.get(0).unwrap().into_val(&env);
        assert_eq!(topic, Symbol::new(&env, "role_revoked"));
        let (emitted_role, emitted_target): (String, Address) = last.2.into_val(&env);
        assert_eq!(emitted_role, role);
        assert_eq!(emitted_target, user);
    }

    #[test]
    fn test_get_role_members_excludes_expired_roles() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        // Grant role with short expiry
        client.grant_role(&admin, &user, &role, &Some(10));

        // Verify user is initially in role members
        let members_before = client.get_role_members(&role);
        assert!(members_before.contains(&user));
        assert_eq!(members_before.len(), 1);

        // Advance time past expiry
        env.ledger().set_timestamp(env.ledger().timestamp() + 20);

        // has_role correctly returns false
        assert!(!client.has_role(&user, &role));

        // get_role_members should not contain the expired user
        let members_after = client.get_role_members(&role);
        assert!(!members_after.contains(&user));
        assert!(members_after.is_empty());
    }

    #[test]
    fn test_get_role_admin_returns_address_after_set() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let role_admin = Address::generate(&env);

        client.set_role_admin(&admin, &role, &role_admin);

        assert_eq!(client.get_role_admin(&role), Some(role_admin));
    }

    #[test]
    fn test_get_role_admin_returns_none_when_not_set() {
        let (env, _admin, client) = setup();
        let role = String::from_str(&env, "operator");

        assert_eq!(client.get_role_admin(&role), None);
    }

    #[test]
    fn test_set_role_admin_unauthorized_fails() {
        let (env, _admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let attacker = Address::generate(&env);
        let target = Address::generate(&env);
        let result = client.try_set_role_admin(&attacker, &role, &target);
        assert_eq!(result, Err(Ok(AccessError::Unauthorized)));
    }

    #[test]
    fn test_get_role_expiry_returns_timestamp() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        let now = env.ledger().timestamp();
        client.grant_role(&admin, &user, &role, &Some(100));
        let expiry = client.get_role_expiry(&role, &user);
        assert_eq!(expiry, Some(now + 100));
    }

    #[test]
    fn test_get_role_expiry_none_when_not_granted() {
        let (env, _admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        assert_eq!(client.get_role_expiry(&role, &user), None);
    }

    #[test]
    fn test_get_role_expiry_max_when_no_expiry() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        client.grant_role(&admin, &user, &role, &None);
        assert_eq!(client.get_role_expiry(&role, &user), Some(u64::MAX));
    }

    #[test]
    fn test_grant_role_batch_all_succeed() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let (u1, u2) = (Address::generate(&env), Address::generate(&env));
        let accounts = vec![&env, u1.clone(), u2.clone()];
        let result = client.grant_role_batch(&admin, &accounts, &role, &None, &false);
        assert_eq!(result.successes.len(), 2);
        assert_eq!(result.failures.len(), 0);
        assert!(client.has_role(&u1, &role));
        assert!(client.has_role(&u2, &role));
    }

    #[test]
    fn test_grant_role_batch_partial_errors() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let u1 = Address::generate(&env);
        let u2 = Address::generate(&env);
        client.grant_role(&admin, &u1, &role, &None);
        let accounts = vec![&env, u1.clone(), u2.clone(), u1.clone()];
        let result = client.grant_role_batch(&admin, &accounts, &role, &None, &false);
        assert_eq!(result.successes.len(), 1);
        assert_eq!(result.successes.get(0).unwrap().index, 1);
        assert_eq!(result.failures.len(), 2);
        assert_eq!(result.failures.get(0).unwrap().index, 0);
        assert_eq!(
            result.failures.get(0).unwrap().error,
            router_common::BatchItemError::AlreadyExists
        );
    }

    #[test]
    fn test_grant_role_batch_fail_fast_stops_early() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let u1 = Address::generate(&env);
        client.grant_role(&admin, &u1, &role, &None);
        let u2 = Address::generate(&env);
        let accounts = vec![&env, u1.clone(), u2.clone()];
        let result = client.grant_role_batch(&admin, &accounts, &role, &None, &true);
        assert_eq!(result.successes.len(), 0);
        assert_eq!(result.failures.len(), 1);
        assert!(!client.has_role(&u2, &role));
    }

    // ── Issue #578: list_all_roles ────────────────────────────────────────────

    #[test]
    fn test_list_all_roles_empty_initially() {
        let (env, _admin, client) = setup();
        let roles = client.list_all_roles();
        assert!(roles.is_empty());
    }

    #[test]
    fn test_list_all_roles_tracks_roles_from_grant() {
        let (env, admin, client) = setup();
        let role1 = String::from_str(&env, "operator");
        let role2 = String::from_str(&env, "editor");
        let user = Address::generate(&env);

        client.grant_role(&admin, &user, &role1, &None);
        let roles_after_first = client.list_all_roles();
        assert_eq!(roles_after_first.len(), 1);
        assert!(roles_after_first.contains(&role1));

        client.grant_role(&admin, &user, &role2, &None);
        let roles_after_second = client.list_all_roles();
        assert_eq!(roles_after_second.len(), 2);
        assert!(roles_after_second.contains(&role1));
        assert!(roles_after_second.contains(&role2));
    }

    #[test]
    fn test_list_all_roles_tracks_roles_from_set_role_admin() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "viewer");
        let role_admin = Address::generate(&env);

        // set_role_admin with a brand-new role should track it
        client.set_role_admin(&admin, &role, &role_admin);
        let roles = client.list_all_roles();
        assert!(roles.contains(&role));
    }

    #[test]
    fn test_list_all_roles_deduplicates() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user1 = Address::generate(&env);
        let user2 = Address::generate(&env);

        // Grant the same role to two different users — role should only appear once
        client.grant_role(&admin, &user1, &role, &None);
        client.grant_role(&admin, &user2, &role, &None);

        let roles = client.list_all_roles();
        assert_eq!(roles.len(), 1);
        assert!(roles.contains(&role));
    }

    #[test]
    fn test_list_all_roles_persists_after_revoke() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        client.grant_role(&admin, &user, &role, &None);
        client.revoke_role(&admin, &role, &user);

        // Role should still appear in list_all_roles even after all members are revoked
        let roles = client.list_all_roles();
        assert!(roles.contains(&role));
    }

    #[test]
    fn test_role_hierarchy_grants_transitive_access() {
        let (env, admin, client) = setup();
        let viewer = String::from_str(&env, "viewer");
        let editor = String::from_str(&env, "editor");
        let owner = String::from_str(&env, "owner");
        let user = Address::generate(&env);

        client.set_role_parent(&admin, &viewer, &editor);
        client.set_role_parent(&admin, &editor, &owner);
        client.grant_role(&admin, &user, &owner, &None);

        assert!(client.has_role(&user, &owner));
        assert!(client.has_role(&user, &editor));
        assert!(client.has_role(&user, &viewer));
        assert_eq!(client.get_role_parent(&viewer), Some(editor));
    }

    #[test]
    fn test_set_role_parent_rejects_transitive_cycle() {
        let (env, admin, client) = setup();
        let role_a = String::from_str(&env, "role-a");
        let role_b = String::from_str(&env, "role-b");
        let role_c = String::from_str(&env, "role-c");

        client.set_role_parent(&admin, &role_a, &role_b);
        client.set_role_parent(&admin, &role_b, &role_c);

        let result = client.try_set_role_parent(&admin, &role_c, &role_a);

        assert_eq!(result, Err(Ok(AccessError::HierarchyCycle)));
        assert_eq!(client.get_role_parent(&role_c), None);
    }
}
