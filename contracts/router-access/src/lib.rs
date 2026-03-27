#![no_std]

//! # router-access
//!
//! Role-based access control for the stellar-router suite.
//! Supports arbitrary roles, multi-admin, and per-address whitelisting.
//!
//! ## Features
//! - Define and grant/revoke named roles
//! - Super-admin can manage all roles
//! - Check role membership on-chain
//! - Whitelist/blacklist individual callers

use soroban_sdk::{contract, contractimpl, contracttype, contracterror, Address, Env, String, Symbol};

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    SuperAdmin,
    HasRole(String, Address),   // (role, address) -> bool
    RoleAdmin(String),          // role -> Address who manages it
    Blacklisted(Address),
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
    ///
    /// Must be called exactly once before any other function. The `super_admin`
    /// address gains full control over all roles and blacklisting.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `super_admin` - The address that will have super-admin privileges.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`AccessError::AlreadyInitialized`] — if the contract has already been initialized.
    pub fn initialize(env: Env, super_admin: Address) -> Result<(), AccessError> {
        if env.storage().instance().has(&DataKey::SuperAdmin) {
            return Err(AccessError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::SuperAdmin, &super_admin);
        Ok(())
    }

    /// Grant a role to an address. Caller must be super-admin or role admin.
    ///
    /// Assigns `role` to `target`. The `target` must not already hold the role
    /// and must not be blacklisted. The `caller` must be either the super-admin
    /// or the designated admin for `role`.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be super-admin or role admin.
    /// * `role` - The name of the role to grant.
    /// * `target` - The address that will receive the role.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`AccessError::Unauthorized`] — if `caller` is neither the super-admin nor the role admin.
    /// * [`AccessError::AlreadyHasRole`] — if `target` already holds `role`.
    /// * [`AccessError::Blacklisted`] — if `target` is blacklisted.
    pub fn grant_role(
        env: Env,
        caller: Address,
        role: String,
        target: Address,
    ) -> Result<(), AccessError> {
        caller.require_auth();
        Self::require_role_manager(&env, &caller, &role)?;

        if Self::has_role_internal(&env, &role, &target) {
            return Err(AccessError::AlreadyHasRole);
        }
        if Self::is_blacklisted_internal(&env, &target) {
            return Err(AccessError::Blacklisted);
        }

        env.storage()
            .instance()
            .set(&DataKey::HasRole(role.clone(), target.clone()), &true);
        env.events().publish(
            (Symbol::new(&env, "role_granted"),),
            (role, target),
        );
        Ok(())
    }

    /// Revoke a role from an address.
    ///
    /// Removes `role` from `target`. The `target` must currently hold the role.
    /// The `caller` must be either the super-admin or the designated admin for `role`.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be super-admin or role admin.
    /// * `role` - The name of the role to revoke.
    /// * `target` - The address whose role will be revoked.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`AccessError::Unauthorized`] — if `caller` is neither the super-admin nor the role admin.
    /// * [`AccessError::RoleNotFound`] — if `target` does not hold `role`.
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
        env.events().publish(
            (Symbol::new(&env, "role_revoked"),),
            (role, target),
        );
        Ok(())
    }

    /// Check if an address has a role.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `role` - The name of the role to check.
    /// * `target` - The address to check.
    ///
    /// # Returns
    /// `true` if `target` holds `role`, `false` otherwise.
    pub fn has_role(env: Env, role: String, target: Address) -> bool {
        Self::has_role_internal(&env, &role, &target)
    }

    /// Set the admin for a specific role (who can grant/revoke it).
    ///
    /// Designates `admin` as the address allowed to grant and revoke `role`.
    /// Only the super-admin can call this function.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the super-admin.
    /// * `role` - The name of the role whose admin is being set.
    /// * `admin` - The address that will manage `role`.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`AccessError::Unauthorized`] — if `caller` is not the super-admin.
    /// * [`AccessError::NotInitialized`] — if the contract has not been initialized.
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

    /// Blacklist an address — prevents it from being granted any role.
    ///
    /// Once blacklisted, `target` cannot be passed to `grant_role`.
    /// The super-admin itself cannot be blacklisted. Only the super-admin can
    /// call this function.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the super-admin.
    /// * `target` - The address to blacklist.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`AccessError::Unauthorized`] — if `caller` is not the super-admin.
    /// * [`AccessError::CannotBlacklistAdmin`] — if `target` is the super-admin.
    /// * [`AccessError::NotInitialized`] — if the contract has not been initialized.
    pub fn blacklist(env: Env, caller: Address, target: Address) -> Result<(), AccessError> {
        caller.require_auth();
        Self::require_super_admin(&env, &caller)?;

        // Cannot blacklist the super admin
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

    /// Remove an address from the blacklist.
    ///
    /// Allows `target` to be granted roles again after being blacklisted.
    /// Only the super-admin can call this function.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the super-admin.
    /// * `target` - The address to remove from the blacklist.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`AccessError::Unauthorized`] — if `caller` is not the super-admin.
    /// * [`AccessError::NotInitialized`] — if the contract has not been initialized.
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

    /// Check if an address is blacklisted.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `target` - The address to check.
    ///
    /// # Returns
    /// `true` if `target` is blacklisted, `false` otherwise.
    pub fn is_blacklisted(env: Env, target: Address) -> bool {
        Self::is_blacklisted_internal(&env, &target)
    }

    /// Transfer super-admin to a new address.
    ///
    /// Replaces the current super-admin with `new_admin`. The `current` address
    /// must authenticate and must be the existing super-admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `current` - The current super-admin address; must authenticate.
    /// * `new_admin` - The address that will become the new super-admin.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`AccessError::Unauthorized`] — if `current` is not the super-admin.
    /// * [`AccessError::NotInitialized`] — if the contract has not been initialized.
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

    /// Get current super-admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// The [`Address`] of the current super-admin.
    ///
    /// # Errors
    /// * [`AccessError::NotInitialized`] — if the contract has not been initialized.
    pub fn super_admin(env: Env) -> Result<Address, AccessError> {
        env.storage()
            .instance()
            .get(&DataKey::SuperAdmin)
            .ok_or(AccessError::NotInitialized)
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
        // Super admin can always manage roles
        if let Some(admin) = env.storage().instance().get::<DataKey, Address>(&DataKey::SuperAdmin) {
            if &admin == caller {
                return Ok(());
            }
        }
        // Role-specific admin
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

    fn has_role_internal(env: &Env, role: &String, target: &Address) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::HasRole(role.clone(), target.clone()))
            .unwrap_or(false)
    }

    fn is_blacklisted_internal(env: &Env, target: &Address) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Blacklisted(target.clone()))
            .unwrap_or(false)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        vec,
        Env,
        IntoVal,
        String,
        Val,
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

    #[test]
    fn test_grant_and_check_role() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        client.grant_role(&admin, &role, &user);
        assert!(client.has_role(&role, &user));
    }

    #[test]
    fn test_grant_role_emits_event() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);

        client.grant_role(&admin, &role, &user);

        let event = env.events().all().last().unwrap().clone();
        assert_eq!(event.0, client.address);
        assert_eq!(event.1, vec![&env, Symbol::new(&env, "role_granted").into_val(&env)]);
        let expected_data: Val = (role, user).into_val(&env);
        assert_eq!(event.2, expected_data);
    }

    #[test]
    fn test_revoke_role() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        client.grant_role(&admin, &role, &user);
        client.revoke_role(&admin, &role, &user);
        assert!(!client.has_role(&role, &user));
    }

    #[test]
    fn test_revoke_role_emits_event() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        client.grant_role(&admin, &role, &user);

        client.revoke_role(&admin, &role, &user);

        let event = env.events().all().last().unwrap().clone();
        assert_eq!(event.0, client.address);
        assert_eq!(event.1, vec![&env, Symbol::new(&env, "role_revoked").into_val(&env)]);
        let expected_data: Val = (role, user).into_val(&env);
        assert_eq!(event.2, expected_data);
    }

    #[test]
    fn test_double_grant_fails() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        client.grant_role(&admin, &role, &user);
        let result = client.try_grant_role(&admin, &role, &user);
        assert_eq!(result, Err(Ok(AccessError::AlreadyHasRole)));
    }

    #[test]
    fn test_blacklist_prevents_grant() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        client.blacklist(&admin, &user);
        let result = client.try_grant_role(&admin, &role, &user);
        assert_eq!(result, Err(Ok(AccessError::Blacklisted)));
    }

    #[test]
    fn test_blacklist_emits_event() {
        let (env, admin, client) = setup();
        let user = Address::generate(&env);

        client.blacklist(&admin, &user);

        let event = env.events().all().last().unwrap().clone();
        assert_eq!(event.0, client.address);
        assert_eq!(
            event.1,
            vec![&env, Symbol::new(&env, "address_blacklisted").into_val(&env)]
        );
        let expected_data: Val = user.into_val(&env);
        assert_eq!(event.2, expected_data);
    }

    #[test]
    fn test_cannot_blacklist_admin() {
        let (env, admin, client) = setup();
        let result = client.try_blacklist(&admin, &admin);
        assert_eq!(result, Err(Ok(AccessError::CannotBlacklistAdmin)));
    }

    #[test]
    fn test_role_admin_can_grant() {
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let role_admin = Address::generate(&env);
        let user = Address::generate(&env);
        client.set_role_admin(&admin, &role, &role_admin);
        client.grant_role(&role_admin, &role, &user);
        assert!(client.has_role(&role, &user));
    }

    #[test]
    fn test_unauthorized_grant_fails() {
        let (env, _admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let attacker = Address::generate(&env);
        let user = Address::generate(&env);
        let result = client.try_grant_role(&attacker, &role, &user);
        assert_eq!(result, Err(Ok(AccessError::Unauthorized)));
    }

    #[test]
    fn test_transfer_super_admin() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_super_admin(&admin, &new_admin);
        assert_eq!(client.super_admin(), new_admin);
    }

    #[test]
    fn test_transfer_super_admin_emits_event() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);

        client.transfer_super_admin(&admin, &new_admin);

        let event = env.events().all().last().unwrap().clone();
        assert_eq!(event.0, client.address);
        assert_eq!(
            event.1,
            vec![&env, Symbol::new(&env, "admin_transferred").into_val(&env)]
        );
        let expected_data: Val = (admin, new_admin).into_val(&env);
        assert_eq!(event.2, expected_data);
    }

    #[test]
    fn test_blacklist_address_with_no_role_succeeds() {
        // Blacklisting an address that has no role should succeed silently
        let (env, admin, client) = setup();
        let user = Address::generate(&env);
        // This should not panic or return an error
        client.blacklist(&admin, &user);
        assert!(client.is_blacklisted(&user));
    }

    #[test]
    fn test_super_admin_cannot_grant_to_blacklisted() {
        // Even the super admin cannot grant a role to a blacklisted address
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        client.blacklist(&admin, &user);
        let result = client.try_grant_role(&admin, &role, &user);
        assert_eq!(result, Err(Ok(AccessError::Blacklisted)));
    }

    #[test]
    fn test_blacklist_address_with_role_revokes_and_blocks_future_grants() {
        // Blacklisting an address that has a role should:
        // 1. Allow the blacklisting to succeed
        // 2. Block future grant attempts
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        
        // First, grant the role
        client.grant_role(&admin, &role, &user);
        assert!(client.has_role(&role, &user));
        
        // Blacklist the user - this should succeed
        client.blacklist(&admin, &user);
        assert!(client.is_blacklisted(&user));
        
        // Attempt to grant the role again (should fail because they already have it)
        let result = client.try_grant_role(&admin, &role, &user);
        assert_eq!(result, Err(Ok(AccessError::AlreadyHasRole)));
        
        // Revoke the role
        client.revoke_role(&admin, &role, &user);
        assert!(!client.has_role(&role, &user));
        
        // Now try to grant again - should fail because blacklisted
        let result = client.try_grant_role(&admin, &role, &user);
        assert_eq!(result, Err(Ok(AccessError::Blacklisted)));
    }

    #[test]
    fn test_unblacklist_allows_role_grant() {
        // Removing an address from the blacklist should allow role grants again
        let (env, admin, client) = setup();
        let role = String::from_str(&env, "operator");
        let user = Address::generate(&env);
        
        // Blacklist the user
        client.blacklist(&admin, &user);
        assert!(client.is_blacklisted(&user));
        
        // Attempt to grant should fail
        let result = client.try_grant_role(&admin, &role, &user);
        assert_eq!(result, Err(Ok(AccessError::Blacklisted)));
        
        // Remove from blacklist
        client.unblacklist(&admin, &user);
        let contract_id = client.address.clone();
        let blacklist_key_present = env.as_contract(&contract_id, || {
            env.storage().instance().has(&DataKey::Blacklisted(user.clone()))
        });
        assert!(!blacklist_key_present);
        assert!(!client.is_blacklisted(&user));
        
        // Now grant should succeed
        client.grant_role(&admin, &role, &user);
        assert!(client.has_role(&role, &user));
    }

    #[test]
    fn test_unblacklist_emits_event() {
        let (env, admin, client) = setup();
        let user = Address::generate(&env);
        client.blacklist(&admin, &user);

        client.unblacklist(&admin, &user);

        let event = env.events().all().last().unwrap().clone();
        assert_eq!(event.0, client.address);
        assert_eq!(
            event.1,
            vec![&env, Symbol::new(&env, "address_unblacklisted").into_val(&env)]
        );
        let expected_data: Val = user.into_val(&env);
        assert_eq!(event.2, expected_data);
    }
}
