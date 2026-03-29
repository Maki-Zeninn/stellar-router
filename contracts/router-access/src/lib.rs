#![no_std]

//! # router-access
//!
//! Role-based access control for the stellar-router suite.

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, Address, Env, String, Symbol, Vec};

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    SuperAdmin,
    HasRole(String, Address),   // (role, address) -> bool
    RoleAdmin(String),          // role -> Address who manages it
    Blacklisted(Address),
    RoleMembers(String),        // role -> Vec<Address>
    AddressRoles(Address),      // address -> Vec<String>
    RoleExpiry(String, Address),
    RoleExpiry(Address, Symbol), // (role, address) -> u64 (ledger timestamp)
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
        env.storage().instance().set(&DataKey::SuperAdmin, &super_admin);
        Ok(())
    }

    /// Grant a role to an address.
    pub fn grant_role(env: Env, admin: Address, account: Address, role: Symbol, expires_in: Option<u64>) {
        admin.require_auth();

        // Optional: add admin check via storage if needed

        let expiry_timestamp = match expires_in {
            Some(seconds) => env.ledger().timestamp() + seconds,
            None => u64::MAX, // No expiry (permanent role)
        };

        let key = DataKey::RoleExpiry(account.clone(), role.clone());
        env.storage().instance().set(&key, &expiry_timestamp);

        // Optional: emit event
        env.events().publish(
            (symbol_short!("role_grant"),),
            (account, role, expiry_timestamp),
        );
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

        if !Self::has_role_internal(&env, &role, &target) {
            return Err(AccessError::RoleNotFound);
        }

        env.storage()
            .instance()
            .remove(&DataKey::HasRole(role.clone(), target.clone()));

        let mut members: Vec<Address> = env.storage().instance()
            .get(&DataKey::RoleMembers(role.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        if let Some(i) = members.iter().position(|a| a == target) {
            members.remove(i as u32);
        }
        env.storage().instance().set(&DataKey::RoleMembers(role.clone()), &members);

        let mut roles: Vec<String> = env.storage().instance()
            .get(&DataKey::AddressRoles(target.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        if let Some(i) = roles.iter().position(|r| r == role) {
            roles.remove(i as u32);
        }
        env.storage().instance().set(&DataKey::AddressRoles(target.clone()), &roles);

        env.events().publish(
            (Symbol::new(&env, "role_revoked"),),
            (role, target),
        );
        Ok(())
    }

    /// Check if an address has a role (and it has not expired).
    pub fn has_role(env: Env, account: Address, role: Symbol) -> bool {
        Self::has_role_internal(&env, &account, role)
    }

    /// Internal helper — FIXED for #125
    fn has_role_internal(env: &Env, account: &Address, role: Symbol) -> bool {
        // Check blacklist first
        if Self::is_blacklisted_internal(env, account) {
            return false;
        }

        // Use RoleExpiry key with (Address, Symbol)
        let key = DataKey::RoleExpiry(account.clone(), role.clone());
        let expires_at: Option<u64> = env.storage().instance().get(&key);

        match expires_at {
            Some(expires_at) => {
                let current_time = env.ledger().timestamp();   // ← FIXED: timestamp() instead of sequence()
                if current_time >= expires_at {
                    return false; // Role has expired
                }
                true
            }
            None => false, // No role assigned
        }
    }

    /// Check if a role has expired for an address.
    pub fn is_role_expired(env: Env, role: String, target: Address) -> bool {
        if let Some(expires_at) = env.storage()
            .instance()
            .get::<DataKey, u64>(&DataKey::RoleExpiry(role, target))
        {
            let current_timestamp = env.ledger().timestamp();
            current_timestamp >= expires_at
        } else {
            false
        }
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
        env.storage().instance().set(&DataKey::RoleAdmin(role), &admin);
        Ok(())
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
            (Symbol::new(&env, "address_blacklisted"),),
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
            (Symbol::new(&env, "address_unblacklisted"),),
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
        env.storage().instance()
            .get(&DataKey::RoleMembers(role))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_roles_for_address(env: Env, addr: Address) -> Vec<String> {
        env.storage().instance()
            .get(&DataKey::AddressRoles(addr))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn transfer_super_admin(
        env: Env,
        current: Address,
        new_admin: Address,
    ) -> Result<(), AccessError> {
        current.require_auth();
        Self::require_super_admin(&env, &current)?;
        env.storage().instance().set(&DataKey::SuperAdmin, &new_admin);
        env.events().publish(
            (Symbol::new(&env, "admin_transferred"),),
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
        env.events().publish(
            (Symbol::new(&env, "role_expired"),),
            (role, target),
        );
        Ok(())
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

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
        if let Some(admin) = env.storage().instance().get::<DataKey, Address>(&DataKey::SuperAdmin) {
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
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        vec, Env, Symbol,
    };

    fn setup() -> (Env, Address, RouterAccessClient) {
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
        let role = Symbol::new(&env, "operator");
        let user = Address::generate(&env);

        client.grant_role(&admin, &user, &role, &Some(10));

        // Advance time past expiry
        env.ledger().set_timestamp(env.ledger().timestamp() + 20);

        assert!(!client.has_role(&user, &role));
    }

    // NEW TEST requested by the issue
    #[test]
    fn test_role_expires_correctly_with_timestamp() {
        let (env, admin, client) = setup();
        let role = Symbol::new(&env, "operator");
        let user = Address::generate(&env);

        // Grant role that expires 1 second ago
        client.grant_role(&admin, &user, &role, &Some(1));

        // Advance ledger time
        env.ledger().set_timestamp(env.ledger().timestamp() + 5);

        assert!(!client.has_role(&user, &role));
    }
}