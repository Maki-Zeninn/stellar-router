# Issue #1671 — `transfer_super_admin` Hand-Rolls Storage Set and Event That `admin_transfer_complete!` Provides

**Tracking:** [#1671](https://github.com/Maki-Zeninn/stellar-router/issues/1671)  
**Component:** `contracts/router-access`  
**Type:** Refactor  
**Impact:** Code inconsistency — the same two-step pattern is maintained in two places instead of one; future changes to the shared macro won't automatically apply here

---

## Summary

`transfer_super_admin` ends with an explicit storage write followed by a manual event
publish. That two-step sequence is exactly what the `router_common::admin_transfer_complete!`
macro expands to. Every other contract that transfers admin ownership — `router-quote`,
`router-execution`, `router-multicall`, `router-middleware`, and `router-timelock` (fixed
in #1525) — already uses the macro. `router-access` is the only outlier.

---

## Location

**File:** `contracts/router-access/src/lib.rs`, around line 621

```rust
// Current code
pub fn transfer_super_admin(
    env: Env,
    current: Address,
    new_admin: Address,
) -> Result<(), AccessError> {
    router_common::extend_instance_ttl(&env, INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
    current.require_auth();
    Self::require_super_admin(&env, &current)?;
    if Self::is_blacklisted_internal(&env, &new_admin) {
        return Err(AccessError::Blacklisted);
    }
    env.storage()
        .instance()
        .set(&DataKey::SuperAdmin, &new_admin);  // ← hand-rolled
    env.events().publish(                          // ← hand-rolled
        (Symbol::new(&env, router_common::EVENT_ADMIN_TRANSFERRED),),
        (current, new_admin),
    );
    Ok(())
}
```

**Macro definition:** `contracts/router-common/src/lib.rs`

```rust
macro_rules! admin_transfer_complete {
    ($env:expr, $current:expr, $new_admin:expr, $key:expr) => {
        $env.storage().instance().set($key, $new_admin);
        $env.events().publish(
            (soroban_sdk::Symbol::new($env, $crate::EVENT_ADMIN_TRANSFERRED),),
            ($current.clone(), $new_admin.clone()),
        );
    };
}
```

The two hand-rolled statements in `transfer_super_admin` are equivalent to:

```rust
router_common::admin_transfer_complete!(&env, &current, &new_admin, &DataKey::SuperAdmin);
```

---

## Why This Matters

If `admin_transfer_complete!` is updated — for example to add a second event field,
change the topic structure, or bump the event name — that change applies automatically
to every caller of the macro. `transfer_super_admin` would be silently left behind
unless someone remembers to update it separately. The inconsistency also makes
the codebase harder to audit: a reviewer grepping for all admin-transfer sites won't
find this one with a search for the macro name.

---

## Required Change

Replace the two statements at the end of `transfer_super_admin` with the macro call.
The blacklist check before them must stay in place.

```rust
// After fix
pub fn transfer_super_admin(
    env: Env,
    current: Address,
    new_admin: Address,
) -> Result<(), AccessError> {
    router_common::extend_instance_ttl(&env, INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
    current.require_auth();
    Self::require_super_admin(&env, &current)?;
    if Self::is_blacklisted_internal(&env, &new_admin) {
        return Err(AccessError::Blacklisted);
    }
    router_common::admin_transfer_complete!(&env, &current, &new_admin, &DataKey::SuperAdmin);
    Ok(())
}
```

This is a pure substitution: behaviour, storage key, event name, and event payload are
identical before and after the change.

---

## Tests Required

No new tests are needed. All existing `transfer_super_admin` tests must pass unchanged,
confirming that the macro expansion is behaviourally equivalent to the hand-rolled code.

Existing test coverage includes:

- Successful transfer updates `DataKey::SuperAdmin` and emits `admin_transferred`.
- Transfer to self succeeds.
- Transfer by a non-admin fails.
- Transfer to a blacklisted address is rejected.

---

## Acceptance Criteria

- [ ] `transfer_super_admin` uses `admin_transfer_complete!` instead of the two
  hand-rolled statements.
- [ ] The blacklist check remains before the macro call.
- [ ] All existing `transfer_super_admin` tests pass without modification.
