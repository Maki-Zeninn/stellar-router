# Issue #1673 — `resolve_with_dependencies` Doesn't Follow Aliases and Skips the `NotInitialized` Check

**Tracking:** [#1673](https://github.com/Maki-Zeninn/stellar-router/issues/1673)  
**Component:** `contracts/router-core`  
**Type:** Bug  
**Impact:** Alias names always return `RouteNotFound` from `resolve_with_dependencies`, and an uninitialized contract returns `RouteNotFound` instead of `NotInitialized`

---

## Summary

`resolve` and `resolve_with_dependencies` are meant to be equivalent entry points that
differ only in whether they return a single address or the full dependency chain.
Two pre-flight checks present in `resolve` are absent from `resolve_with_dependencies`:

1. **`NotInitialized` guard** — `resolve` checks for `DataKey::Admin` and returns
   `RouterError::NotInitialized` when the contract has not been set up. Without this
   guard, `resolve_with_dependencies` returns `RouteNotFound` on an uninitialized
   contract, which is a different and misleading error.

2. **Alias resolution** — `resolve` looks up `DataKey::Alias(name)` and substitutes
   the original route name before the lookup. `resolve_with_dependencies` checks
   directly for `DataKey::Route(name)` without consulting the alias table, so an alias
   name that works with `resolve` always produces `RouteNotFound` from
   `resolve_with_dependencies`.

---

## Location

**File:** `contracts/router-core/src/lib.rs`

### `resolve` — does both checks (lines ~982–1010)

```rust
pub fn resolve(env: Env, name: String) -> Result<Address, RouterError> {
    router_common::extend_instance_ttl(&env, INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);

    // ✅ NotInitialized guard
    if !env.storage().instance().has(&DataKey::Admin) {
        return Err(RouterError::NotInitialized);
    }

    let paused: bool = env.storage().instance()
        .get(&DataKey::Paused).unwrap_or(false);
    if paused {
        return Err(RouterError::RouterPaused);
    }

    // ✅ Alias resolution
    let (resolved_name, _alias_used) = if let Some(original) = env
        .storage().instance()
        .get::<DataKey, String>(&DataKey::Alias(name.clone()))
    {
        (original, true)
    } else {
        (name.clone(), false)
    };

    // ... continues with resolved_name
}
```

### `resolve_with_dependencies` — missing both checks (lines ~1305–1340)

```rust
pub fn resolve_with_dependencies(
    env: Env,
    name: String,
) -> Result<Vec<(String, Address)>, RouterError> {
    router_common::extend_instance_ttl(&env, INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);

    let paused: bool = env.storage().instance()
        .get(&DataKey::Paused).unwrap_or(false);
    if paused {
        return Err(RouterError::RouterPaused);
    }

    // ❌ No NotInitialized guard — uninitialized contract falls through to RouteNotFound

    // ❌ No alias resolution — alias names always fail here
    if !env.storage().instance().has(&DataKey::Route(name.clone())) {
        return Err(RouterError::RouteNotFound);
    }

    // ...
}
```

---

## Observed vs Expected Behaviour

**Alias scenario** (`set_alias("oracle-alias", "oracle-v2")`):

| Call | Expected | Actual |
|---|---|---|
| `resolve("oracle-alias")` | `Ok(address)` | `Ok(address)` ✅ |
| `resolve_with_dependencies("oracle-alias")` | `Ok([(…, address)])` | `Err(RouteNotFound)` ❌ |

**Uninitialized contract scenario:**

| Call | Expected | Actual |
|---|---|---|
| `resolve("any-route")` | `Err(NotInitialized)` | `Err(NotInitialized)` ✅ |
| `resolve_with_dependencies("any-route")` | `Err(NotInitialized)` | `Err(RouteNotFound)` ❌ |

---

## Required Change

Add both missing checks to `resolve_with_dependencies`, immediately after the TTL
extension and before the `Paused` check. The simplest approach is a shared private
helper that both functions call, so the two checks can't diverge again:

```rust
// Suggested private helper
fn require_initialized(env: &Env) -> Result<(), RouterError> {
    if !env.storage().instance().has(&DataKey::Admin) {
        return Err(RouterError::NotInitialized);
    }
    Ok(())
}

fn resolve_name(env: &Env, name: String) -> String {
    env.storage()
        .instance()
        .get::<DataKey, String>(&DataKey::Alias(name.clone()))
        .unwrap_or(name)
}
```

```rust
pub fn resolve_with_dependencies(
    env: Env,
    name: String,
) -> Result<Vec<(String, Address)>, RouterError> {
    router_common::extend_instance_ttl(&env, INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);

    // ✅ Add NotInitialized guard
    Self::require_initialized(&env)?;

    let paused: bool = env.storage().instance()
        .get(&DataKey::Paused).unwrap_or(false);
    if paused {
        return Err(RouterError::RouterPaused);
    }

    // ✅ Add alias resolution
    let resolved_name = Self::resolve_name(&env, name);

    if !env.storage().instance().has(&DataKey::Route(resolved_name.clone())) {
        return Err(RouterError::RouteNotFound);
    }

    let mut resolved: Vec<(String, Address)> = Vec::new(&env);
    let mut resolved_names: Vec<String> = Vec::new(&env);
    let mut active_stack: Vec<String> = Vec::new(&env);
    Self::resolve_dependencies_recursive(
        &env,
        &resolved_name,
        &mut active_stack,
        &mut resolved_names,
        &mut resolved,
        0,
    )?;

    Ok(resolved)
}
```

---

## Tests Required

**Alias resolves with dependencies:**
```rust
#[test]
fn test_resolve_with_dependencies_follows_alias() {
    let (env, admin, client) = setup();
    // Register a route and set an alias for it
    client.register_route(&admin, &String::from_str(&env, "oracle-v2"), &addr, &None);
    client.set_alias(&admin, &String::from_str(&env, "oracle-alias"),
                              &String::from_str(&env, "oracle-v2"));
    let result = client.resolve_with_dependencies(&String::from_str(&env, "oracle-alias"));
    assert!(result.is_ok());
    assert!(!result.unwrap().is_empty());
}
```

**Uninitialized contract returns `NotInitialized`:**
```rust
#[test]
fn test_resolve_with_dependencies_on_uninitialized_returns_not_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RouterCore);
    let client = RouterCoreClient::new(&env, &contract_id);
    let result = client.try_resolve_with_dependencies(&String::from_str(&env, "any"));
    assert_eq!(result, Err(Ok(RouterError::NotInitialized)));
}
```

---

## Acceptance Criteria

- [ ] `resolve_with_dependencies` returns `RouterError::NotInitialized` when the
  contract has not been initialized, matching `resolve`.
- [ ] `resolve_with_dependencies` resolves alias names to their target route before
  looking up the dependency chain, matching `resolve`.
- [ ] Tests cover both the alias case and the uninitialized case.
- [ ] No existing tests are broken.
