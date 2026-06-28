#![no_std]

//! # router-registry
//!
//! Central registry for the stellar-router suite.
//! Stores contract addresses keyed by name + version, supports deprecation and lookup.
//!
//! ## Features
//! - Register contracts by name and semantic version
//! - Lookup latest or specific version of a contract
//! - Deprecate old versions
//! - Admin-controlled with ownership transfer

extern crate alloc;
use alloc::string::ToString;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, String, Symbol, Val, Vec,
};
use router_common;

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Admin,
    Entry(String, u32),    // (name, version) -> ContractEntry
    Versions(String),      // name -> Vec<u32>
    ContractNames,         // Vec<String> of all registered names
    AddressIndex(Address), // address -> (name, version) for O(1) reverse lookup
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ContractEntry {
    /// Registered contract address
    pub address: Address,
    /// Human-readable name
    pub name: String,
    /// Version number (monotonically increasing)
    pub version: u32,
    /// Whether this entry has been deprecated
    pub deprecated: bool,
    /// Who registered it
    pub registered_by: Address,
    /// Optional reason recorded when this entry was deprecated
    pub deprecation_reason: Option<String>,
}

/// Input for a single entry in [`RouterRegistry::bulk_register`].
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BulkRegistrationInput {
    pub name: String,
    pub address: Address,
    pub version: u32,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RegistryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    NotFound = 4,
    AlreadyRegistered = 5,
    AlreadyDeprecated = 6,
    InvalidVersion = 7,
    VersionNotFound = 8,
    InvalidConstraint = 9,
    AllVersionsDeprecated = 10,
    ContractUnreachable = 11,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct RouterRegistry;

#[contractimpl]
impl RouterRegistry {
    /// Initialize the registry with an admin address.
    ///
    /// Must be called exactly once before any other function. Sets the admin
    /// who controls all write operations on the registry.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `admin` - The address that will have admin privileges over this registry.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`RegistryError::AlreadyInitialized`] — if the contract has already been initialized.
    pub fn initialize(env: Env, admin: Address) -> Result<(), RegistryError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(RegistryError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    /// Register a new contract entry.
    ///
    /// Stores a [`ContractEntry`] keyed by `(name, version)`. The `version`
    /// must be greater than all previously registered versions for the same
    /// `name` and must be non-zero. Caller must be the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `name` - A human-readable identifier for the contract.
    /// * `address` - The contract address to register.
    /// * `version` - A monotonically increasing version number (must be > 0 and
    ///   greater than any existing version for `name`).
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`RegistryError::Unauthorized`] — if `caller` is not the admin.
    /// * [`RegistryError::InvalidVersion`] — if `version` is 0 or not greater than all existing versions.
    /// * [`RegistryError::AlreadyRegistered`] — if `(name, version)` is already registered.
    /// * [`RegistryError::NotInitialized`] — if the contract has not been initialized.
    pub fn register(
        env: Env,
        caller: Address,
        name: String,
        address: Address,
        version: u32,
    ) -> Result<(), RegistryError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        Self::register_entry(&env, &caller, name, address, version)
    }

    /// Register a new contract entry with an optional liveness check.
    ///
    /// When `health_fn` is provided, the registry invokes that function on
    /// `address` via a cross-contract call before storing the entry. If the
    /// call fails, registration is rejected with
    /// [`RegistryError::ContractUnreachable`].
    ///
    /// When `health_fn` is `None`, this behaves identically to [`register`].
    pub fn register_with_check(
        env: Env,
        caller: Address,
        name: String,
        version: u32,
        address: Address,
        health_fn: Option<Symbol>,
    ) -> Result<(), RegistryError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if let Some(fn_sym) = health_fn {
            let args: Vec<Val> = Vec::new(&env);
            if env
                .try_invoke_contract::<Val, Val>(&address, &fn_sym, args)
                .is_err()
            {
                return Err(RegistryError::ContractUnreachable);
            }
        }

        Self::register_entry(&env, &caller, name, address, version)
    }

    /// Register multiple contract entries in one call.
    pub fn bulk_register(
        env: Env,
        caller: Address,
        entries: Vec<BulkRegistrationInput>,
        fail_fast: bool,
    ) -> Result<router_common::BatchResult, RegistryError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        let mut result = router_common::BatchResult::new(&env);

        if fail_fast {
            for (index, entry) in entries.iter().enumerate() {
                let idx = index as u32;
                if let Err(err) = Self::validate_registration(&env, &entry.name, entry.version) {
                    result.record_failure(&env, idx, Self::registry_error_message(err));
                    return Ok(result);
                }
            }
            for (index, entry) in entries.iter().enumerate() {
                Self::register_entry(
                    &env,
                    &caller,
                    entry.name.clone(),
                    entry.address.clone(),
                    entry.version,
                )?;
                result.record_success(index as u32);
            }
        } else {
            for (index, entry) in entries.iter().enumerate() {
                let idx = index as u32;
                match Self::validate_registration(&env, &entry.name, entry.version) {
                    Ok(()) => match Self::register_entry(
                        &env,
                        &caller,
                        entry.name.clone(),
                        entry.address.clone(),
                        entry.version,
                    ) {
                        Ok(()) => result.record_success(idx),
                        Err(err) => {
                            result.record_failure(&env, idx, Self::registry_error_message(err));
                        }
                    },
                    Err(err) => {
                        result.record_failure(&env, idx, Self::registry_error_message(err));
                    }
                }
            }
        }

        Ok(result)
    }

    /// Look up a contract by name and specific version.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `name` - The human-readable name of the contract.
    /// * `version` - The exact version number to retrieve.
    ///
    /// # Returns
    /// The [`ContractEntry`] for `(name, version)`.
    ///
    /// # Errors
    /// * [`RegistryError::NotFound`] — if no entry exists for `(name, version)`.
    pub fn get(env: Env, name: String, version: u32) -> Result<ContractEntry, RegistryError> {
        env.storage()
            .instance()
            .get(&DataKey::Entry(name, version))
            .ok_or(RegistryError::NotFound)
    }

    /// Get the latest (highest version) non-deprecated entry for a name.
    ///
    /// Iterates registered versions in descending order and returns the first
    /// entry that has not been deprecated.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `name` - The human-readable name of the contract.
    ///
    /// # Returns
    /// The most recent non-deprecated [`ContractEntry`] for `name`.
    ///
    /// # Errors
    /// * [`RegistryError::NotFound`] — if no non-deprecated entry exists for `name`.
    pub fn get_latest(env: Env, name: String) -> Result<ContractEntry, RegistryError> {
        let versions = Self::get_versions_list(&env, &name);
        if versions.is_empty() {
            return Err(RegistryError::NotFound);
        }
        // Iterate in reverse to find latest non-deprecated
        let len = versions.len();
        let mut i = len;
        while i > 0 {
            i -= 1;
            let v = versions.get(i).ok_or(RegistryError::NotFound)?;
            let entry: ContractEntry = env
                .storage()
                .instance()
                .get(&DataKey::Entry(name.clone(), v))
                .ok_or(RegistryError::NotFound)?;
            if !entry.deprecated {
                return Ok(entry);
            }
        }
        Err(RegistryError::AllVersionsDeprecated)
    }

    /// Get the latest non-deprecated entry matching a semver constraint.
    ///
    /// Accepts an optional semver constraint string (e.g., ">=2.0,<3.0" or "^1.5").
    /// Returns the highest non-deprecated version satisfying the constraint.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `name` - The human-readable name of the contract.
    /// * `constraint` - Optional semver constraint (e.g., ">=2.0,<3.0").
    ///
    /// # Returns
    /// The most recent non-deprecated [`ContractEntry`] matching the constraint.
    ///
    /// # Errors
    /// * [`RegistryError::NotFound`] — if no matching entry exists.
    /// * [`RegistryError::InvalidConstraint`] — if constraint format is invalid.
    pub fn get_latest_with_constraint(
        env: Env,
        name: String,
        constraint: Option<String>,
    ) -> Result<ContractEntry, RegistryError> {
        let versions = Self::get_versions_list(&env, &name);

        // If no constraint, use get_latest logic
        if constraint.is_none() {
            let len = versions.len();
            let mut i = len;
            while i > 0 {
                i -= 1;
                let v = versions.get(i).ok_or(RegistryError::NotFound)?;
                let entry: ContractEntry = env
                    .storage()
                    .instance()
                    .get(&DataKey::Entry(name.clone(), v))
                    .ok_or(RegistryError::NotFound)?;
                if !entry.deprecated {
                    return Ok(entry);
                }
            }
            return Err(RegistryError::AllVersionsDeprecated);
        }

        let constraint_str = constraint.unwrap();

        Self::validate_constraint(&constraint_str)?;

        if versions.is_empty() {
            return Err(RegistryError::NotFound);
        }

        let mut any_constraint_match = false;

        // Iterate in reverse to find latest matching non-deprecated version
        let len = versions.len();
        let mut i = len;
        while i > 0 {
            i -= 1;
            let v = versions.get(i).ok_or(RegistryError::NotFound)?;
            let entry: ContractEntry = env
                .storage()
                .instance()
                .get(&DataKey::Entry(name.clone(), v))
                .ok_or(RegistryError::NotFound)?;
            if Self::version_matches_constraint(v, &constraint_str)? {
                any_constraint_match = true;
            } else {
                continue;
            }

            if !entry.deprecated {
                return Ok(entry);
            }
        }
        if any_constraint_match {
            Err(RegistryError::AllVersionsDeprecated)
        } else {
            Err(RegistryError::NotFound)
        }
    }

    /// Deprecate a specific version of a contract.
    ///
    /// Marks the entry for `(name, version)` as deprecated so it will be
    /// skipped by `get_latest`. Caller must be the admin.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `caller` - The address initiating the call; must be the admin.
    /// * `name` - The human-readable name of the contract.
    /// * `version` - The version number to deprecate.
    /// * `reason` - Optional human-readable reason for the deprecation.
    ///
    /// # Returns
    /// `Ok(())` on success.
    ///
    /// # Errors
    /// * [`RegistryError::Unauthorized`] — if `caller` is not the admin.
    /// * [`RegistryError::NotFound`] — if no entry exists for `(name, version)`.
    /// * [`RegistryError::AlreadyDeprecated`] — if the entry is already deprecated.
    /// * [`RegistryError::NotInitialized`] — if the contract has not been initialized.
    pub fn deprecate(
        env: Env,
        caller: Address,
        name: String,
        version: u32,
        reason: Option<String>,
    ) -> Result<(), RegistryError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        Self::deprecate_one(&env, name, version, reason)
    }

    fn deprecate_one(
        env: &Env,
        name: String,
        version: u32,
        reason: Option<String>,
    ) -> Result<(), RegistryError> {
        let mut entry: ContractEntry = env
            .storage()
            .instance()
            .get(&DataKey::Entry(name.clone(), version))
            .ok_or(RegistryError::VersionNotFound)?;

        if entry.deprecated {
            return Err(RegistryError::AlreadyDeprecated);
        }

        entry.deprecated = true;
        entry.deprecation_reason = reason;
        env.storage()
            .instance()
            .set(&DataKey::Entry(name.clone(), version), &entry);
        env.events()
            .publish((Symbol::new(&env, router_common::EVENT_CONTRACT_DEPRECATED),), (name, version));
        Ok(())
    }

    pub fn deprecate_many(
        env: Env,
        caller: Address,
        entries: Vec<(String, u32)>,
        fail_fast: bool,
    ) -> Result<router_common::BatchResult, RegistryError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        let mut result = router_common::BatchResult::new(&env);
        for (index, (name, version)) in entries.iter().enumerate() {
            let idx = index as u32;
            match Self::deprecate_one(&env, name.clone(), version, None) {
                Ok(()) => result.record_success(idx),
                Err(err) => {
                    result.record_failure(&env, idx, Self::registry_error_message(err));
                    if fail_fast {
                        break;
                    }
                }
            }
        }
        Ok(result)
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
    /// * [`RegistryError::Unauthorized`] — if `current` is not the admin.
    /// * [`RegistryError::NotInitialized`] — if the contract has not been initialized.
    pub fn transfer_admin(
        env: Env,
        current: Address,
        new_admin: Address,
    ) -> Result<(), RegistryError> {
        current.require_auth();
        Self::require_admin(&env, &current)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.events().publish(
            (Symbol::new(&env, router_common::EVENT_ADMIN_TRANSFERRED),),
            (current, new_admin),
        );
        Ok(())
    }

    /// Get the current admin.
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
    /// # Errors
    /// * [`RegistryError::NotInitialized`] — if the contract has not been initialized.
    pub fn admin(env: Env) -> Result<Address, RegistryError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RegistryError::NotInitialized)
    }

    /// Get all registered versions for a name.
    ///
    /// Returns the list of version numbers that have been registered under
    /// `name`, in the order they were registered (ascending).
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `name` - The human-readable name of the contract.
    ///
    /// # Returns
    /// A [`Vec<u32>`] of version numbers. Returns an empty vector if `name`
    /// has no registered versions.
    pub fn versions(env: Env, name: String) -> Vec<u32> {
        // Invariant: versions for a given `name` are stored in ascending order.
        //
        // This is guaranteed by `register_entry()` enforcing that each new
        // `version` is strictly greater than all existing versions for the
        // same name and then appending to the end of the stored list.
        Self::get_versions_list(&env, &name)
    }



    /// Get all contract entries for a name.

    ///
    /// Returns all [`ContractEntry`] structs for `name`, including deprecated ones,
    /// in version order (ascending).
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `name` - The human-readable name of the contract.
    ///
    /// # Returns
    /// A [`Vec<ContractEntry>`] of all entries for `name`.
    pub fn get_all_versions(env: Env, name: String) -> Vec<ContractEntry> {
        let versions = Self::get_versions_list(&env, &name);
        let mut entries = Vec::new(&env);
        for v in versions.iter() {
            if let Some(entry) = env
                .storage()
                .instance()
                .get(&DataKey::Entry(name.clone(), v))
            {
                entries.push_back(entry);
            }
        }
        entries
    }

    /// Returns all registered contract names in registration order.
    ///
    /// Returns a list of all unique contract names that have been registered,
    /// in the order they were first registered.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    ///
    /// # Returns
    /// A [`Vec<String>`] of all registered contract names.
    pub fn get_all_names(env: Env) -> Vec<String> {
        env.storage()
            .instance()
            .get(&DataKey::ContractNames)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Find the registry entry for a given contract address (O(1) reverse lookup).
    ///
    /// Uses the address index written by `register()` to look up the entry
    /// in constant time. Returns `None` if the address is not registered.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment.
    /// * `address` - The contract address to look up.
    ///
    /// # Returns
    /// An [`Option<ContractEntry>`] containing the entry if found, `None` otherwise.
    pub fn get_entry_by_address(env: Env, address: Address) -> Option<ContractEntry> {
        let (name, version): (String, u32) = env
            .storage()
            .instance()
            .get(&DataKey::AddressIndex(address))?;
        env.storage()
            .instance()
            .get(&DataKey::Entry(name, version))
        let (name, version) = env
            .storage()
            .instance()
            .get::<DataKey, (String, u32)>(&DataKey::AddressIndex(address))?;
        env.storage().instance().get(&DataKey::Entry(name, version))
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn registry_error_message(err: RegistryError) -> &'static str {
        match err {
            RegistryError::AlreadyInitialized => "AlreadyInitialized",
            RegistryError::NotInitialized => "NotInitialized",
            RegistryError::Unauthorized => "Unauthorized",
            RegistryError::NotFound => "NotFound",
            RegistryError::AlreadyRegistered => "AlreadyRegistered",
            RegistryError::AlreadyDeprecated => "AlreadyDeprecated",
            RegistryError::InvalidVersion => "InvalidVersion",
            RegistryError::VersionNotFound => "VersionNotFound",
            RegistryError::InvalidConstraint => "InvalidConstraint",
            RegistryError::AllVersionsDeprecated => "AllVersionsDeprecated",
            RegistryError::ContractUnreachable => "ContractUnreachable",
        }
    }

    fn validate_registration(env: &Env, name: &String, version: u32) -> Result<(), RegistryError> {
        if version == 0 {
            return Err(RegistryError::InvalidVersion);
        }
        if env
            .storage()
            .instance()
            .has(&DataKey::Entry(name.clone(), version))
        {
            return Err(RegistryError::AlreadyRegistered);
        }
        let versions = Self::get_versions_list(env, name);
        for v in versions.iter() {
            if version <= v {
                return Err(RegistryError::InvalidVersion);
            }
        }
        Ok(())
    }

    fn register_entry(
        env: &Env,
        caller: &Address,
        name: String,
        address: Address,
        version: u32,
    ) -> Result<(), RegistryError> {
        Self::validate_registration(env, &name, version)?;

        let entry = ContractEntry {
            address: address.clone(),
            name: name.clone(),
            version,
            deprecated: false,
            registered_by: caller.clone(),
            deprecation_reason: None,
        };

        env.storage()
            .instance()
            .set(&DataKey::Entry(name.clone(), version), &entry);

        let mut versions = Self::get_versions_list(env, &name);
        versions.push_back(version);
        env.storage()
            .instance()
            .set(&DataKey::Versions(name.clone()), &versions);

        let mut names: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::ContractNames)
            .unwrap_or_else(|| Vec::new(env));
        if !names.contains(&name) {
            names.push_back(name.clone());
            env.storage()
                .instance()
                .set(&DataKey::ContractNames, &names);
        }

        env.storage()
            .instance()
            .set(&DataKey::AddressIndex(address), &(name.clone(), version));

        env.events()
            .publish((Symbol::new(env, router_common::EVENT_CONTRACT_REGISTERED),), (name, version));

        Ok(())
    }

    fn require_admin(env: &Env, caller: &Address) -> Result<(), RegistryError> {
        let admin = Self::admin(env.clone())?;
        if &admin != caller {
            return Err(RegistryError::Unauthorized);
        }
        Ok(())
    }

    fn get_versions_list(env: &Env, name: &String) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&DataKey::Versions(name.clone()))
            .unwrap_or(Vec::new(env))
    }

    /// Validate that a constraint string uses a supported format before use.
    ///
    /// Supported formats: `>=X`, `<=X`, `>X`, `<X`, `^X`, `~X`, or an exact
    /// version number `X`, where `X` is a non-negative integer.
    ///
    /// # Errors
    /// * [`RegistryError::InvalidConstraint`] — if the format is not recognised.
    fn validate_constraint(constraint: &String) -> Result<(), RegistryError> {
        let s = constraint.to_string();
        let num_part = if s.starts_with(">=") || s.starts_with("<=") {
            &s[2..]
        } else if s.starts_with('>')
            || s.starts_with('<')
            || s.starts_with('^')
            || s.starts_with('~')
        {
            &s[1..]
        } else {
            s.as_str()
        };

        if num_part.is_empty() || num_part.parse::<u32>().is_err() {
            return Err(RegistryError::InvalidConstraint);
        }
        Ok(())
    }

    fn version_matches_constraint(
        version: u32,
        constraint: &String,
    ) -> Result<bool, RegistryError> {
        // Parse simple semver constraints: >=X, <=X, >X, <X, ^X, ~X
        let constraint_str = constraint.to_string();

        if constraint_str.starts_with(">=") {
            let min = constraint_str[2..]
                .parse::<u32>()
                .map_err(|_| RegistryError::InvalidConstraint)?;
            Ok(version >= min)
        } else if constraint_str.starts_with("<=") {
            let max = constraint_str[2..]
                .parse::<u32>()
                .map_err(|_| RegistryError::InvalidConstraint)?;
            Ok(version <= max)
        } else if constraint_str.starts_with(">") {
            let min = constraint_str[1..]
                .parse::<u32>()
                .map_err(|_| RegistryError::InvalidConstraint)?;
            Ok(version > min)
        } else if constraint_str.starts_with("<") {
            let max = constraint_str[1..]
                .parse::<u32>()
                .map_err(|_| RegistryError::InvalidConstraint)?;
            Ok(version < max)
        } else if constraint_str.starts_with("^") {
            // Caret: allows changes that do not modify the left-most non-zero digit
            let base = constraint_str[1..]
                .parse::<u32>()
                .map_err(|_| RegistryError::InvalidConstraint)?;
            if base == 0 {
                Ok(version >= base && version < 1)
            } else {
                Ok(version >= base && version < base + 1)
            }
        } else if constraint_str.starts_with("~") {
            // Tilde: allows patch-level changes
            let base = constraint_str[1..]
                .parse::<u32>()
                .map_err(|_| RegistryError::InvalidConstraint)?;
            Ok(version >= base && version < base + 1)
        } else {
            // Try exact match
            let exact = constraint_str
                .parse::<u32>()
                .map_err(|_| RegistryError::InvalidConstraint)?;
            Ok(version == exact)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        vec, Env, IntoVal, String,
    };

    fn setup() -> (Env, Address, RouterRegistryClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterRegistry);
        let client = RouterRegistryClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    #[test]
    fn test_initialize() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterRegistry);
        let client = RouterRegistryClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let result = client.try_initialize(&admin);
        assert!(result.is_ok());
    }

    #[test]
    fn test_double_initialize_fails() {
        let (_, admin, client) = setup();
        let result = client.try_initialize(&admin);
        assert_eq!(result, Err(Ok(RegistryError::AlreadyInitialized)));
    }

    #[test]
    fn test_register_and_get() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        let entry = client.get(&name, &1);
        assert_eq!(entry.address, addr);
        assert_eq!(entry.version, 1);
        assert!(!entry.deprecated);
    }

    #[test]
    fn test_get_latest() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);
        client.register(&admin, &name, &addr1, &1);
        client.register(&admin, &name, &addr2, &2);
        let latest = client.get_latest(&name);
        assert_eq!(latest.address, addr2);
        assert_eq!(latest.version, 2);
    }

    #[test]
    fn test_deprecate() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);
        client.register(&admin, &name, &addr1, &1);
        client.register(&admin, &name, &addr2, &2);
        client.deprecate(&admin, &name, &2, &None::<String>);
        client.deprecate(&admin, &name, &2, &None);
        // latest should now return v1
        let latest = client.get_latest(&name);
        assert_eq!(latest.version, 1);
    }

    #[test]
    fn test_deprecate_nonexistent_version_returns_error() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        let result = client.try_deprecate(&admin, &name, &99, &None::<String>);
        let result = client.try_deprecate(&admin, &name, &99, &None);
        assert_eq!(result, Err(Ok(RegistryError::VersionNotFound)));
    }

    #[test]
    fn test_get_latest_unknown_name_returns_not_found() {
        let (env, _admin, client) = setup();
        let name = String::from_str(&env, "unknown");
        let result = client.try_get_latest(&name);
        assert_eq!(result, Err(Ok(RegistryError::NotFound)));
    }

    #[test]
    fn test_get_latest_returns_not_found_when_all_deprecated() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        client.deprecate(&admin, &name, &1, &None::<String>);
        client.deprecate(&admin, &name, &1, &None);
        let result = client.try_get_latest(&name);
        assert_eq!(result, Err(Ok(RegistryError::AllVersionsDeprecated)));
    }

    #[test]
    fn test_get_latest_skips_multiple_deprecated_versions() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);
        client.deprecate(&admin, &name, &3, &None::<String>);
        client.deprecate(&admin, &name, &2, &None::<String>);
        client.deprecate(&admin, &name, &3, &None);
        client.deprecate(&admin, &name, &2, &None);
        let latest = client.get_latest(&name);
        assert_eq!(latest.version, 1);
    }

    #[test]
    fn test_duplicate_version_fails() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        let result = client.try_register(&admin, &name, &addr, &1);
        assert_eq!(result, Err(Ok(RegistryError::AlreadyRegistered)));
    }

    #[test]
    fn test_version_must_increase() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &5);
        let result = client.try_register(&admin, &name, &addr, &3);
        assert_eq!(result, Err(Ok(RegistryError::InvalidVersion)));
    }

    #[test]
    fn test_unauthorized_register_fails() {
        let (env, _admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        let attacker = Address::generate(&env);
        let result = client.try_register(&attacker, &name, &addr, &1);
        assert_eq!(result, Err(Ok(RegistryError::Unauthorized)));
    }

    #[test]
    fn test_transfer_admin() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);
        client.transfer_admin(&admin, &new_admin);
        assert_eq!(client.admin(), new_admin);
    }

    #[test]
    fn test_register_emits_event() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);

        let event = env.events().all().last().unwrap().clone();
        assert_eq!(event.0, client.address);
        assert_eq!(
            event.1,
            vec![
                &env,
                Symbol::new(&env, "contract_registered").into_val(&env)
            ]
        );
        let (n, v): (String, u32) = event.2.into_val(&env);
        assert_eq!(n, name);
        assert_eq!(v, 1u32);
    }

    #[test]
    fn test_deprecate_emits_event() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        client.deprecate(&admin, &name, &1, &None::<String>);
        client.deprecate(&admin, &name, &1, &None);

        let event = env.events().all().last().unwrap().clone();
        assert_eq!(event.0, client.address);
        assert_eq!(
            event.1,
            vec![
                &env,
                Symbol::new(&env, "contract_deprecated").into_val(&env)
            ]
        );
        let (n, v): (String, u32) = event.2.into_val(&env);
        assert_eq!(n, name);
        assert_eq!(v, 1u32);
    }

    #[test]
    fn test_transfer_admin_emits_event() {
        let (env, admin, client) = setup();
        let new_admin = Address::generate(&env);

        client.transfer_admin(&admin, &new_admin);

        let event = env.events().all().last().unwrap().clone();
        assert_eq!(event.0, client.address);
        assert_eq!(
            event.1,
            vec![&env, Symbol::new(&env, "admin_transferred").into_val(&env)]
        );
        let (old, new): (Address, Address) = event.2.into_val(&env);
        assert_eq!(old, admin);
        assert_eq!(new, new_admin);
    }

    #[test]
    fn test_register_higher_after_deprecation_succeeds() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        client.deprecate(&admin, &name, &1, &None::<String>);
        client.deprecate(&admin, &name, &1, &None);

        // Registering a higher version after deprecation should succeed
        client.register(&admin, &name, &addr, &2);
        let latest = client.get_latest(&name);
        assert_eq!(latest.version, 2);
    }

    #[test]
    fn test_get_latest_all_deprecated_fails() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        client.register(&admin, &name, &addr, &2);
        client.deprecate(&admin, &name, &1, &None::<String>);
        client.deprecate(&admin, &name, &2, &None::<String>);
        client.deprecate(&admin, &name, &1, &None);
        client.deprecate(&admin, &name, &2, &None);

        // When all versions are deprecated, get_latest should return NotFound
        let result = client.try_get_latest(&name);
        assert_eq!(result, Err(Ok(RegistryError::AllVersionsDeprecated)));
    }

    #[test]
    fn test_get_latest_unknown_name_fails() {
        let (env, _admin, client) = setup();
        let name = String::from_str(&env, "nonexistent");
        let result = client.try_get_latest(&name);
        assert_eq!(result, Err(Ok(RegistryError::NotFound)));
    }

    #[test]
    fn test_versions_unknown_name_returns_empty() {
        let (env, _admin, client) = setup();
        let name = String::from_str(&env, "nonexistent");
        let versions = client.versions(&name);
        assert!(versions.is_empty());
    }

    #[test]
    fn test_deprecate_many_all_succeed() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);

        let entries = vec![
            &env,
            (name.clone(), 1u32),
            (name.clone(), 2u32),
            (name.clone(), 3u32),
        ];
        let results = client.deprecate_many(&admin, &entries, &false);
        assert_eq!(results.successes.len(), 3);
        assert_eq!(results.failures.len(), 0);
    }

    #[test]
    fn test_deprecate_many_partial_errors() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);

        let entries = vec![
            &env,
            (name.clone(), 1u32),  // ok
            (name.clone(), 99u32), // VersionNotFound
            (name.clone(), 1u32),  // AlreadyDeprecated
        ];
        let results = client.deprecate_many(&admin, &entries, &false);
        assert_eq!(results.successes.len(), 1);
        assert_eq!(results.successes.get(0).unwrap().index, 0);
        assert_eq!(results.failures.len(), 2);
        assert_eq!(results.failures.get(0).unwrap().index, 1);
        assert_eq!(
            results.failures.get(0).unwrap().message,
            String::from_str(&env, "VersionNotFound")
        );
        assert_eq!(results.failures.get(1).unwrap().index, 2);
        assert_eq!(
            results.failures.get(1).unwrap().message,
            String::from_str(&env, "AlreadyDeprecated")
        );
    }

    #[test]
    fn test_bulk_register_all_succeed() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2) = (Address::generate(&env), Address::generate(&env));
        let a2_clone = a2.clone();
        let entries = vec![
            &env,
            BulkRegistrationInput {
                name: name.clone(),
                address: a1,
                version: 1,
            },
            BulkRegistrationInput {
                name: name.clone(),
                address: a2,
                version: 2,
            },
        ];
        let result = client.bulk_register(&admin, &entries, &false);
        assert_eq!(result.successes.len(), 2);
        assert_eq!(result.failures.len(), 0);
        assert_eq!(client.get(&name, &2).address, a2_clone);
    }

    #[test]
    fn test_bulk_register_partial_errors() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);

        let entries = vec![
            &env,
            BulkRegistrationInput {
                name: name.clone(),
                address: Address::generate(&env),
                version: 2,
            },
            BulkRegistrationInput {
                name: name.clone(),
                address: Address::generate(&env),
                version: 0,
            },
            BulkRegistrationInput {
                name: name.clone(),
                address: Address::generate(&env),
                version: 1,
            },
        ];
        let result = client.bulk_register(&admin, &entries, &false);
        assert_eq!(result.successes.len(), 1);
        assert_eq!(result.successes.get(0).unwrap().index, 0);
        assert_eq!(result.failures.len(), 2);
        assert_eq!(
            result.failures.get(0).unwrap().message,
            String::from_str(&env, "InvalidVersion")
        );
        assert_eq!(
            result.failures.get(1).unwrap().message,
            String::from_str(&env, "AlreadyRegistered")
        );
    }

    #[test]
    fn test_bulk_register_fail_fast_stops_early() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let entries = vec![
            &env,
            BulkRegistrationInput {
                name: name.clone(),
                address: Address::generate(&env),
                version: 0,
            },
            BulkRegistrationInput {
                name: name.clone(),
                address: Address::generate(&env),
                version: 1,
            },
        ];
        let result = client.bulk_register(&admin, &entries, &true);
        assert_eq!(result.successes.len(), 0);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures.get(0).unwrap().index, 0);
    }

    #[test]
    fn test_get_latest_with_constraint_exact_match() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);

        let constraint = String::from_str(&env, "2");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert!(result.is_ok());
        let entry = result.unwrap().unwrap();
        assert_eq!(entry.version, 2);
    }

    #[test]
    fn test_get_latest_with_constraint_gte() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);

        let constraint = String::from_str(&env, ">=2");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert!(result.is_ok());
        let entry = result.unwrap().unwrap();
        assert_eq!(entry.version, 3);
    }

    #[test]
    fn test_get_latest_with_constraint_lt() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);

        let constraint = String::from_str(&env, "<3");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert!(result.is_ok());
        let entry = result.unwrap().unwrap();
        assert_eq!(entry.version, 2);
    }

    #[test]
    fn test_get_latest_with_constraint_caret() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);

        let constraint = String::from_str(&env, "^2");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert!(result.is_ok());
        let entry = result.unwrap().unwrap();
        assert_eq!(entry.version, 2);
    }

    #[test]
    fn test_get_latest_with_constraint_no_match() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2) = (Address::generate(&env), Address::generate(&env));
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);

        let constraint = String::from_str(&env, ">=5");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert_eq!(result, Err(Ok(RegistryError::NotFound)));
    }

    #[test]
    fn test_get_latest_with_constraint_skips_deprecated() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);
        client.deprecate(&admin, &name, &3, &None::<String>);
        client.deprecate(&admin, &name, &3, &None);

        let constraint = String::from_str(&env, ">=2");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert!(result.is_ok());
        let entry = result.unwrap().unwrap();
        assert_eq!(entry.version, 2);
    }

    #[test]
    fn test_get_latest_with_constraint_all_deprecated_returns_all_versions_deprecated() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);
        client.deprecate(&admin, &name, &1, &None::<String>);
        client.deprecate(&admin, &name, &2, &None::<String>);
        client.deprecate(&admin, &name, &3, &None::<String>);
        client.deprecate(&admin, &name, &1, &None);
        client.deprecate(&admin, &name, &2, &None);
        client.deprecate(&admin, &name, &3, &None);
        let constraint = String::from_str(&env, ">=1");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert_eq!(result, Err(Ok(RegistryError::AllVersionsDeprecated)));
    }

    #[test]
    fn test_get_latest_with_constraint_empty_registry() {
        let (env, _admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let constraint = String::from_str(&env, ">=1");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert_eq!(result, Err(Ok(RegistryError::NotFound)));
    }

    #[test]
    fn test_get_latest_with_constraint_invalid_constraint() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);

        let bad_constraint = String::from_str(&env, "abc");
        let result = client.try_get_latest_with_constraint(&name, &Some(bad_constraint));
        assert_eq!(result, Err(Ok(RegistryError::InvalidConstraint)));
    }

    #[test]
    fn test_get_latest_with_constraint_none_behaves_like_get_latest() {
        // Verifies the no-constraint path returns the same result as get_latest
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        let latest = client.get_latest(&name);
        let constrained = client.get_latest_with_constraint(&name, &None);
        assert_eq!(latest.version, constrained.version);
        assert_eq!(latest.address, constrained.address);
    }

    #[test]
    fn test_constraint_all_deprecated_returns_all_deprecated() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        client.register(&admin, &name, &addr, &2);
        client.register(&admin, &name, &addr, &3);
        client.deprecate(&admin, &name, &1, &None::<String>);
        client.deprecate(&admin, &name, &2, &None::<String>);
        client.deprecate(&admin, &name, &3, &None::<String>);
        client.deprecate(&admin, &name, &1, &None);
        client.deprecate(&admin, &name, &2, &None);
        client.deprecate(&admin, &name, &3, &None);

        let constraint = String::from_str(&env, ">=1");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert_eq!(result, Err(Ok(RegistryError::AllVersionsDeprecated)));
    }

    #[test]
    fn test_constraint_no_matching_version_returns_not_found() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2) = (Address::generate(&env), Address::generate(&env));
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);

        let constraint = String::from_str(&env, ">=5");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert_eq!(result, Err(Ok(RegistryError::NotFound)));
    }

    #[test]
    fn test_get_all_names_empty() {
        let (env, _admin, client) = setup();
        let names = client.get_all_names();
        assert!(names.is_empty());
    }

    #[test]
    fn test_get_all_names_multiple() {
        let (env, admin, client) = setup();
        let name1 = String::from_str(&env, "oracle");
        let name2 = String::from_str(&env, "vault");
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        client.register(&admin, &name1, &addr1, &1);
        client.register(&admin, &name2, &addr2, &1);

        let names = client.get_all_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&name1));
        assert!(names.contains(&name2));
    }

    #[test]
    fn test_get_all_names_no_duplicates() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        client.register(&admin, &name, &addr1, &1);
        client.register(&admin, &name, &addr2, &2);

        let names = client.get_all_names();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&name));
    }

    #[test]
    fn test_get_all_versions_includes_deprecated() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let a1 = Address::generate(&env);
        let a2 = Address::generate(&env);
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.deprecate(&admin, &name, &1, &None::<String>);
        client.deprecate(&admin, &name, &1, &None);

        let entries = client.get_all_versions(&name);
        assert_eq!(entries.len(), 2);
        let v1 = entries.iter().find(|e| e.version == 1).unwrap();
        let v2 = entries.iter().find(|e| e.version == 2).unwrap();
        assert!(v1.deprecated);
        assert!(!v2.deprecated);
    }

    #[test]
    fn test_get_all_versions_empty_for_unknown_name() {
        let (env, _admin, client) = setup();
        let name = String::from_str(&env, "unknown");
        assert!(client.get_all_versions(&name).is_empty());
    }

    #[test]
    fn test_deprecate_all_versions() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);

        client.deprecate_all_versions(&admin, &name, &None::<String>);

        let result = client.try_get_latest(&name);
        assert_eq!(result, Err(Ok(RegistryError::AllVersionsDeprecated)));
        // All entries should be marked deprecated
        for v in [1u32, 2, 3] {
            assert!(client.get(&name, &v).deprecated);
        }
    }

    #[test]
    fn test_deprecate_all_versions_skips_already_deprecated() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        // Pre-deprecate v1
        client.deprecate(&admin, &name, &1, &None::<String>);
        // deprecate_all_versions must not error on already-deprecated entries
        assert!(client
            .try_deprecate_all_versions(&admin, &name, &None::<String>)
            .is_ok());
    }

    #[test]
    fn test_deprecate_with_reason_stored_and_emitted() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        let reason = String::from_str(&env, "security vulnerability in v1");
        client.register(&admin, &name, &addr, &1);
        client.deprecate(&admin, &name, &1, &Some(reason.clone()));

        let entry = client.get(&name, &1);
        assert!(entry.deprecated);
        assert_eq!(entry.deprecation_reason, Some(reason.clone()));

        let event = env.events().all().last().unwrap().clone();
        let (n, v, r): (String, u32, Option<String>) = event.2.into_val(&env);
        assert_eq!(n, name);
        assert_eq!(v, 1u32);
        assert_eq!(r, Some(reason));
    }

    #[test]
    fn test_get_latest_with_constraint_version_zero_matching() {
        // version 0 is not allowed to be registered, but constraint >=0 should match version 1
    fn test_get_entry_by_address_found() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);

        let constraint = String::from_str(&env, ">=0");
        let result = client.get_latest_with_constraint(&name, &Some(constraint));
        assert_eq!(result.version, 1);
    }

    #[test]
    fn test_deprecate_all_versions_unauthorized_fails() {
        let (env, _admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let attacker = Address::generate(&env);
        let result = client.try_deprecate_all_versions(&attacker, &name, &None::<String>);
        assert_eq!(result, Err(Ok(RegistryError::Unauthorized)));
    }

    #[test]
    fn test_get_entry_by_address_found() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        let entry = client.get_entry_by_address(&addr).unwrap();
        assert_eq!(entry.address, addr);
        assert_eq!(entry.name, name);
        assert_eq!(entry.version, 1);
    }

    #[test]
    fn test_get_entry_by_address_not_found() {
        let (env, _admin, client) = setup();
        let unknown = Address::generate(&env);
        assert!(client.get_entry_by_address(&unknown).is_none());
    }

    #[test]
    fn test_deprecate_stores_reason() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        let reason = String::from_str(&env, "security vulnerability");
        client.register(&admin, &name, &addr, &1);
        client.deprecate(&admin, &name, &1, &Some(reason.clone()));
        let entry = client.get(&name, &1);
        assert!(entry.deprecated);
        assert_eq!(entry.deprecation_reason, Some(reason));
    }

    #[test]
    fn test_get_latest_with_constraint_large_version() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        let large_v = u32::MAX;
        client.register(&admin, &name, &addr, &large_v);

        let constraint = String::from_str(&env, "4294967295"); // u32::MAX
        let result = client.get_latest_with_constraint(&name, &Some(constraint));
        assert_eq!(result.version, large_v);

        let gte_constraint = String::from_str(&env, ">=4294967295");
        let result_gte = client.get_latest_with_constraint(&name, &Some(gte_constraint));
        assert_eq!(result_gte.version, large_v);
    }

    #[test]
    fn test_get_latest_with_constraint_caret_zero() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);

        // ^0 means >=0 and <1. Since version 1 is registered, ^0 should NOT match.
        let constraint = String::from_str(&env, "^0");
        let result = client.try_get_latest_with_constraint(&name, &Some(constraint));
        assert_eq!(result, Err(Ok(RegistryError::NotFound)));
    }

    #[test]
    fn test_get_entry_by_address_none_after_deprecate() {
        let reason = String::from_str(&env, "security vulnerability");
        client.deprecate(&admin, &name, &1, &Some(reason.clone()));
        let entry = client.get(&name, &1);
        assert!(entry.deprecated);
        assert_eq!(entry.deprecation_reason, Some(reason));
    }

    #[test]
    fn test_deprecate_without_reason() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        client.deprecate(&admin, &name, &1, &None);
        let entry = client.get(&name, &1);
        assert!(entry.deprecated);
        assert_eq!(entry.deprecation_reason, None);
    }

    // ── register_with_check ───────────────────────────────────────────────────

    #[contract]
    pub struct MockHealthContract;

    #[contractimpl]
    impl MockHealthContract {
        pub fn version(_env: Env) -> u32 {
            1
        }

        pub fn health(_env: Env) {}
    }

    #[test]
    fn test_register_with_check_no_health_fn_succeeds() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        let result = client.try_register_with_check(&admin, &name, &1, &addr, &None::<Symbol>);
        assert_eq!(result, Ok(Ok(())));
        let entry = client.get(&name, &1);
        assert_eq!(entry.address, addr);
    }

    #[test]
    fn test_register_with_check_health_fn_succeeds() {
        let (env, admin, client) = setup();
        let mock_id = env.register_contract(None, MockHealthContract);
        let name = String::from_str(&env, "oracle");
        let health_fn = Symbol::new(&env, "version");
        let result = client.try_register_with_check(&admin, &name, &1, &mock_id, &Some(health_fn));
        assert_eq!(result, Ok(Ok(())));
        let entry = client.get(&name, &1);
        assert_eq!(entry.address, mock_id);
    }

    #[test]
    fn test_register_with_check_missing_health_fn_fails() {
        let (env, admin, client) = setup();
        let mock_id = env.register_contract(None, MockHealthContract);
        let name = String::from_str(&env, "oracle");
        let health_fn = Symbol::new(&env, "nonexistent");
        let result = client.try_register_with_check(&admin, &name, &1, &mock_id, &Some(health_fn));
        assert_eq!(result, Err(Ok(RegistryError::ContractUnreachable)));
        assert_eq!(client.try_get(&name, &1), Err(Ok(RegistryError::NotFound)));
    }

    // ── Issue #644: version constraint parsing edge cases ────────────────────

    #[test]
    fn test_constraint_whitespace_only_is_invalid() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        let result = client.try_get_latest_with_constraint(
            &name,
            &Some(String::from_str(&env, " ")),
        );
        assert_eq!(result, Err(Ok(RegistryError::InvalidConstraint)));
    }

    #[test]
    fn test_constraint_mixed_operator_is_invalid() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        // "~=2" starts with tilde but the rest "=2" is not a valid u32
        let result = client.try_get_latest_with_constraint(
            &name,
            &Some(String::from_str(&env, "~=2")),
        );
        assert_eq!(result, Err(Ok(RegistryError::InvalidConstraint)));
    }

    #[test]
    fn test_constraint_negative_number_is_invalid() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr = Address::generate(&env);
        client.register(&admin, &name, &addr, &1);
        // ">=-1": "-1" cannot parse as u32
        let result = client.try_get_latest_with_constraint(
            &name,
            &Some(String::from_str(&env, ">=-1")),
        );
        assert_eq!(result, Err(Ok(RegistryError::InvalidConstraint)));
    }

    #[test]
    fn test_constraint_equals_prefix_is_invalid() {
        // "=3" is not a supported operator — falls to numeric parse of "=3" → fails
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);
        client.register(&admin, &name, &addr1, &1);
        client.register(&admin, &name, &addr2, &3);
        let result = client.try_get_latest_with_constraint(
            &name,
            &Some(String::from_str(&env, "=3")),
        );
        assert_eq!(result, Err(Ok(RegistryError::InvalidConstraint)));
    }

    #[test]
    fn test_constraint_exact_numeric_match() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3, a4) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);
        client.register(&admin, &name, &a4, &4);

        let result = client.get_latest_with_constraint(
            &name,
            &Some(String::from_str(&env, "3")),
        );
        assert_eq!(result.version, 3);
    }

    #[test]
    fn test_constraint_gt_operator() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);

        // ">2" means version strictly greater than 2 → only 3 qualifies
        let result = client.get_latest_with_constraint(
            &name,
            &Some(String::from_str(&env, ">2")),
        );
        assert_eq!(result.version, 3);
    }

    #[test]
    fn test_constraint_lte_operator() {
        let (env, admin, client) = setup();
        let name = String::from_str(&env, "oracle");
        let (a1, a2, a3) = (
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        );
        client.register(&admin, &name, &a1, &1);
        client.register(&admin, &name, &a2, &2);
        client.register(&admin, &name, &a3, &3);

        // "<=2" returns highest non-deprecated version ≤ 2 → version 2
        let result = client.get_latest_with_constraint(
            &name,
            &Some(String::from_str(&env, "<=2")),
        );
        assert_eq!(result.version, 2);
    }
}
