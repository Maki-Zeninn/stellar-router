#![no_std]

//! # router-core
//!
//! Central dispatcher for the stellar-router suite.
//! Routes calls to registered contracts by name, enforces access control,
//! and delegates to the registry for address resolution.
//!
//! ## Features
//! - Route calls to contracts by name (resolved via registry)
//! - Admin-controlled route registration and removal
//! - Pause/unpause individual routes or all routing
//! - Event emission on every route operation

use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror,
    Address, Env, String, Symbol, Vec,
};

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Admin,
    Route(String),    // name -> RouteEntry
    Paused,
    TotalRouted,
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RouteEntry {
    /// Resolved contract address for this route
    pub address: Address,
    /// Human-readable route name
    pub name: String,
    /// Whether this specific route is paused
    pub paused: bool,
    /// Who last updated this route
    pub updated_by: Address,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RouterError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    RouteNotFound = 4,
    RoutePaused = 5,
    RouterPaused = 6,
    RouteAlreadyExists = 7,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RouterCore;

#[contractimpl]
impl RouterCore {
    /// Initialize the router with an admin address.
    ///
    /// Sets up the admin, marks the router as unpaused, and resets the total
    /// routed counter to zero. Must be called exactly once before any other
    /// function.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `admin` - The address that will have admin privileges over this router.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`RouterError::AlreadyInitialized`] — if the contract has already been initialized.
    pub fn initialize(env: Env, admin: Address) -> Result<(), RouterError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(RouterError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::TotalRouted, &0u64);
        Ok(())
    }

    /// Register a new route by name pointing to a contract address.
    ///
    /// Associates a human-readable `name` with a target contract `address`.
    /// The route starts in an unpaused state. Caller must be the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `name` - A unique human-readable identifier for the route.
    /// * `address` - The contract address this route resolves to.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`RouterError::Unauthorized`] — if `caller` is not the admin.
    /// * [`RouterError::RouteAlreadyExists`] — if a route with `name` already exists.
    /// * [`RouterError::NotInitialized`] — if the contract has not been initialized.
    pub fn register_route(
        env: Env,
        caller: Address,
        name: String,
        address: Address,
    ) -> Result<(), RouterError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if env.storage().instance().has(&DataKey::Route(name.clone())) {
            return Err(RouterError::RouteAlreadyExists);
        }

        let entry = RouteEntry {
            address,
            name: name.clone(),
            paused: false,
            updated_by: caller,
        };
        env.storage().instance().set(&DataKey::Route(name.clone()), &entry);

        env.events().publish(
            (Symbol::new(&env, "route_registered"),),
            name.clone(),
        );

        Ok(())
    }

    /// Update an existing route to point to a new address.
    ///
    /// Replaces the contract address for an existing route. The route must
    /// already exist. Caller must be the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `name` - The name of the route to update.
    /// * `new_address` - The new contract address for this route.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`RouterError::Unauthorized`] — if `caller` is not the admin.
    /// * [`RouterError::RouteNotFound`] — if no route with `name` exists.
    /// * [`RouterError::NotInitialized`] — if the contract has not been initialized.
    pub fn update_route(
        env: Env,
        caller: Address,
        name: String,
        new_address: Address,
    ) -> Result<(), RouterError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let mut entry: RouteEntry = env
            .storage()
            .instance()
            .get(&DataKey::Route(name.clone()))
            .ok_or(RouterError::RouteNotFound)?;

        entry.address = new_address;
        entry.updated_by = caller;
        env.storage().instance().set(&DataKey::Route(name.clone()), &entry);

        env.events().publish(
            (Symbol::new(&env, "route_updated"),),
            name.clone(),
        );

        Ok(())
    }

    /// Remove a route entirely.
    ///
    /// Deletes the route entry for `name` from storage. Caller must be the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `name` - The name of the route to remove.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`RouterError::Unauthorized`] — if `caller` is not the admin.
    /// * [`RouterError::RouteNotFound`] — if no route with `name` exists.
    /// * [`RouterError::NotInitialized`] — if the contract has not been initialized.
    pub fn remove_route(env: Env, caller: Address, name: String) -> Result<(), RouterError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if !env.storage().instance().has(&DataKey::Route(name.clone())) {
            return Err(RouterError::RouteNotFound);
        }

        env.storage().instance().remove(&DataKey::Route(name.clone()));

        env.events().publish(
            (Symbol::new(&env, "route_removed"),),
            name.clone(),
        );

        Ok(())
    }

    /// Resolve a route name to its contract address.
    ///
    /// Looks up the contract address registered under `name`, validates that
    /// neither the router nor the individual route is paused, increments the
    /// total-routed counter, and emits a `routed` event.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `name` - The name of the route to resolve.
    ///
    /// # Returns
    /// The [`Address`] of the contract registered under `name`.
    ///
    /// # Errors
    /// * [`RouterError::RouterPaused`] — if the entire router is paused.
    /// * [`RouterError::RouteNotFound`] — if no route with `name` exists.
    /// * [`RouterError::RoutePaused`] — if the specific route is paused.
    pub fn resolve(env: Env, name: String) -> Result<Address, RouterError> {
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if paused {
            return Err(RouterError::RouterPaused);
        }

        let entry: RouteEntry = env
            .storage()
            .instance()
            .get(&DataKey::Route(name.clone()))
            .ok_or(RouterError::RouteNotFound)?;

        if entry.paused {
            return Err(RouterError::RoutePaused);
        }

        // Increment total routed counter
        let total: u64 = env
            .storage()
            .instance()
            .get(&DataKey::TotalRouted)
            .unwrap_or(0);
        env.storage().instance().set(&DataKey::TotalRouted, &(total + 1));

        env.events().publish(
            (Symbol::new(&env, "routed"),),
            (name.clone(), entry.address.clone()),
        );

        Ok(entry.address)
    }

    /// Pause or unpause a specific route.
    ///
    /// When a route is paused, calls to `resolve` for that route will
    /// return [`RouterError::RoutePaused`]. Caller must be the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `name` - The name of the route to pause or unpause.
    /// * `paused` - `true` to pause the route, `false` to unpause it.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`RouterError::Unauthorized`] — if `caller` is not the admin.
    /// * [`RouterError::RouteNotFound`] — if no route with `name` exists.
    /// * [`RouterError::NotInitialized`] — if the contract has not been initialized.
    pub fn set_route_paused(
        env: Env,
        caller: Address,
        name: String,
        paused: bool,
    ) -> Result<(), RouterError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        let mut entry: RouteEntry = env
            .storage()
            .instance()
            .get(&DataKey::Route(name.clone()))
            .ok_or(RouterError::RouteNotFound)?;

        entry.paused = paused;
        env.storage().instance().set(&DataKey::Route(name.clone()), &entry);

        env.events().publish(
            (Symbol::new(&env, "route_paused"),),
            (name.clone(), paused),
        );

        Ok(())
    }

    /// Pause or unpause the entire router.
    ///
    /// When the router is paused, all calls to `resolve` will return
    /// [`RouterError::RouterPaused`] regardless of individual route state.
    /// Caller must be the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `paused` - `true` to pause the router, `false` to unpause it.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`RouterError::Unauthorized`] — if `caller` is not the admin.
    /// * [`RouterError::NotInitialized`] — if the contract has not been initialized.
    pub fn set_paused(env: Env, caller: Address, paused: bool) -> Result<(), RouterError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        env.storage().instance().set(&DataKey::Paused, &paused);

        env.events().publish(
            (Symbol::new(&env, "router_paused"),),
            paused,
        );

        Ok(())
    }

    /// Get a route entry by name.
    ///
    /// Returns the full [`RouteEntry`] for the given `name`, or `None` if no
    /// such route is registered.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `name` - The name of the route to look up.
    ///
    /// # Returns
    /// `Some(`[`RouteEntry`]`)` if the route exists, `None` otherwise.
    pub fn get_route(env: Env, name: String) -> Option<RouteEntry> {
        env.storage().instance().get(&DataKey::Route(name))
    }

    /// Get the total number of resolved calls.
    ///
    /// Returns the cumulative count of successful `resolve` invocations
    /// since the contract was initialized.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// The total number of times a route has been resolved.
    pub fn total_routed(env: Env) -> u64 {
        env.storage().instance().get(&DataKey::TotalRouted).unwrap_or(0)
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
    /// * [`RouterError::NotInitialized`] — if the contract has not been initialized.
    pub fn admin(env: Env) -> Result<Address, RouterError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RouterError::NotInitialized)
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
    /// * [`RouterError::Unauthorized`] — if `current` is not the admin.
    /// * [`RouterError::NotInitialized`] — if the contract has not been initialized.
    pub fn transfer_admin(env: Env, current: Address, new_admin: Address) -> Result<(), RouterError> {
        current.require_auth();
        Self::require_admin(&env, &current)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

/// Returns all currently registered route names as a vector of strings.
    /// This is a read-only operation that scans all instance storage keys.
    /// The order of returned names is not guaranteed.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// A `Vec<String>` containing all registered route names.
    pub fn get_all_routes(env: Env) -> Vec<String> {
        let mut routes = Vec::new(&env);
        let mut iter = env.storage().instance().iter();
        while let Some(kv) = iter.next() {
            let (key, _) = kv.unwrap();
            if let DataKey::Route(name) = key {
                routes.append(name);
            }
        }
        routes
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn require_admin(env: &Env, caller: &Address) -> Result<(), RouterError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RouterError::NotInitialized)?;
        if &admin != caller {
            return Err(RouterError::Unauthorized);
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Events}, Env, String};

    fn setup() -> (Env, Address, RouterCoreClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterCore);
        let client = RouterCoreClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    #[test]
    fn test_register_and_resolve() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register_route(&admin, &name, &addr);
        let resolved = client.resolve(&name);
        assert_eq!(resolved, addr);
        assert_eq!(client.total_routed(), 1);
    }

    #[test]
    fn test_update_route() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);
        client.register_route(&admin, &name, &addr1);
        client.update_route(&admin, &name, &addr2);
        assert_eq!(client.resolve(&name), addr2);
    }

    #[test]
    fn test_remove_route() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register_route(&admin, &name, &addr);
        client.remove_route(&admin, &name);
        let result = client.try_resolve(&name);
        assert_eq!(result, Err(Ok(RouterError::RouteNotFound)));
    }

    #[test]
    fn test_duplicate_route_fails() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register_route(&admin, &name, &addr);
        let result = client.try_register_route(&admin, &name, &addr);
        assert_eq!(result, Err(Ok(RouterError::RouteAlreadyExists)));
    }

    #[test]
    fn test_pause_route() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register_route(&admin, &name, &addr);
        client.set_route_paused(&admin, &name, &true);
        let result = client.try_resolve(&name);
        assert_eq!(result, Err(Ok(RouterError::RoutePaused)));
    }

    #[test]
    fn test_pause_and_unpause_route() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        
        // Register a route
        client.register_route(&admin, &name, &addr);
        
        // Verify resolve works initially
        let resolved = client.resolve(&name);
        assert_eq!(resolved, addr);
        
        // Pause the route
        client.set_route_paused(&admin, &name, &true);
        
        // Assert that resolve now fails with RoutePaused error
        let result = client.try_resolve(&name);
        assert_eq!(result, Err(Ok(RouterError::RoutePaused)));
        
        // Unpause the route
        client.set_route_paused(&admin, &name, &false);
        
        // Assert that resolve works again
        let resolved = client.resolve(&name);
        assert_eq!(resolved, addr);
    }

    #[test]
    fn test_pause_router() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register_route(&admin, &name, &addr);
        client.set_paused(&admin, &true);
        let result = client.try_resolve(&name);
        assert_eq!(result, Err(Ok(RouterError::RouterPaused)));
    }

    #[test]
    fn test_unauthorized_register_fails() {
        let (env, _admin, client) = setup();
        let attacker = Address::generate(&env);
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        let result = client.try_register_route(&attacker, &name, &addr);
        assert_eq!(result, Err(Ok(RouterError::Unauthorized)));
    }

    #[test]
    fn test_transfer_admin() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);
        assert_eq!(client.admin(), new_admin);
    }

    #[test]
    fn test_set_route_paused_emits_event() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register_route(&admin, &name, &addr);
        
        let events_before = env.events().all().len();
        client.set_route_paused(&admin, &name, &true);
        let events_after = env.events().all().len();
        
        // Verify an event was emitted
        assert_eq!(events_after, events_before + 1);
    }

    #[test]
    fn test_set_paused_emits_event() {
        let (env, admin, client) = setup();
        
        let events_before = env.events().all().len();
        client.set_paused(&admin, &true);
        let events_after = env.events().all().len();
        
        // Verify an event was emitted
        assert_eq!(events_after, events_before + 1);
    fn test_resolve_unknown_route_fails() {
        let (env, _admin, client) = setup();
        let name = String::from_str(&env, "unknown");
        let result = client.try_resolve(&name);
        assert_eq!(result, Err(Ok(RouterError::RouteNotFound)));
    fn test_get_all_routes_empty() {
        let (env, _, client) = setup();
        let routes: Vec<String> = client.get_all_routes();
        assert!(routes.is_empty());
    }

    #[test]
    fn test_get_all_routes_multiple() {
        let (env, admin, client) = setup();
        let oracle = String::from_str(&env, "oracle");
        let vault = String::from_str(&env, "vault");
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);
        client.register_route(&admin, &oracle, &addr1);
        client.register_route(&admin, &vault, &addr2);
        let routes: Vec<String> = client.get_all_routes();
        assert_eq!(routes.len(), 2);
        assert!(routes.contains(&oracle));
        assert!(routes.contains(&vault));
    }
}
