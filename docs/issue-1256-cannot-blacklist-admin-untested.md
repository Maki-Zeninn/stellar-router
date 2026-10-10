# Issue #1256 — `CannotBlacklistAdmin` Error Path Is Never Tested

**Tracking:** [#1256](https://github.com/Maki-Zeninn/stellar-router/issues/1256)  
**Component:** `contracts/router-access`  
**Type:** Testing Gap  
**Impact:** Missing coverage for a safety-critical guard

---

## Summary

The `blacklist()` function in `router-access` contains an explicit guard that prevents the
current super-admin from being blacklisted. If triggered, the function returns
`AccessError::CannotBlacklistAdmin`. This guard is a meaningful safety property — it
ensures the contract can never be used to lock out its own administrator.

However, no test in the test module ever exercises this code path. The guard is present
and correct, but it is invisible to the test suite.

---

## Location

**File:** `contracts/router-access/src/lib.rs`  
**Approximate lines:** 324–357

```rust
pub fn blacklist(env: Env, caller: Address, target: Address) -> Result<(), AccessError> {
    caller.require_auth();
    Self::require_super_admin(&env, &caller)?;

    let super_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::SuperAdmin)
        .ok_or(AccessError::NotInitialized)?;
    if target == super_admin {
        return Err(AccessError::CannotBlacklistAdmin);   // <-- never reached in tests
    }
    // ...
}
```

---

## Why It Matters

The `CannotBlacklistAdmin` guard exists to prevent a self-inflicted denial-of-service
where the super-admin accidentally (or maliciously) blacklists themselves and loses all
administrative control over the contract. Without a test:

- A future refactor that changes the comparison (e.g. checking the *caller* instead of
  the *target*, or reading from the wrong storage key) would silently break this
  protection.
- The safety property cannot be verified by CI, only by manual code inspection.
- The existing blacklist tests all target non-admin addresses, so the admin-path branch
  has zero coverage:
  - `test_blacklisted_address_cannot_use_role`
  - `test_is_blacklisted_reflects_blacklist_state`
  - `test_blacklisted_role_admin_cannot_grant`

---

## Required Change

Add a dedicated test that calls `try_blacklist` with the super-admin as both caller and
target, and asserts the correct error is returned and the admin is not actually
blacklisted afterward.

```rust
#[test]
fn test_blacklist_super_admin_fails() {
    let (env, admin, client) = setup();
    let result = client.try_blacklist(&admin, &admin);
    assert_eq!(result, Err(Ok(AccessError::CannotBlacklistAdmin)));
    assert!(!client.is_blacklisted(&admin));
}
```

Place this test alongside the other `blacklist`-related tests in the `#[cfg(test)]`
module of `contracts/router-access/src/lib.rs`.

---

## Acceptance Criteria

- [ ] A test named `test_blacklist_super_admin_fails` (or equivalent) exists in
      `contracts/router-access/src/lib.rs`.
- [ ] The test calls `client.try_blacklist(&admin, &admin)` (super-admin targeting
      themselves).
- [ ] The test asserts the result is `Err(Ok(AccessError::CannotBlacklistAdmin))`.
- [ ] The test asserts `client.is_blacklisted(&admin)` returns `false`, confirming the
      guard prevented the state change.
- [ ] No existing tests are modified.
