# Issue #1670 — `get_roles_for_address` Returns Expired Roles and Roles of Blacklisted Addresses

**Tracking:** [#1670](https://github.com/Maki-Zeninn/stellar-router/issues/1670)  
**Component:** `contracts/router-access`  
**Type:** Bug  
**Impact:** UIs and callers that rely on `get_roles_for_address` show roles the address cannot actually use

---

## Summary

`get_roles_for_address(addr)` returns the raw `AddressRoles(addr)` vector from storage
with no filtering. Its sibling function `get_role_members(role)` applies
`has_role_internal` before returning, which excludes expired grants and members that
have been blacklisted. The two functions are inconsistent: an address that has had a
role expire, or that has been blacklisted, disappears from `get_role_members` but
continues to appear in `get_roles_for_address`.

---

## Location

**File:** `contracts/router-access/src/lib.rs`

```rust
// get_role_members — filters correctly (lines ~519–535)
pub fn get_role_members(env: Env, role: String) -> Vec<Address> {
    // ...
    let all_members: Vec<Address> = env.storage().instance()
        .get(&DataKey::RoleMembers(role.clone()))
        .unwrap_or_else(|| Vec::new(&env));
    let mut active_members = Vec::new(&env);
    for member in all_members.iter() {
        if Self::has_role_internal(&env, &member, &role) {
            active_members.push_back(member.clone());
        }
    }
    active_members
}

// get_roles_for_address — returns raw storage, no filtering (lines ~537–542)
pub fn get_roles_for_address(env: Env, addr: Address) -> Vec<String> {
    // ...
    env.storage()
        .instance()
        .get(&DataKey::AddressRoles(addr))
        .unwrap_or_else(|| Vec::new(&env))
}
```

---

## Inconsistency

After `grant_role("operator", addr, Some(expiry))` where `expiry` has passed:

| Function | Returns `"operator"` for `addr`? |
|---|---|
| `get_role_members("operator")` | No — filtered by `has_role_internal` |
| `get_roles_for_address(addr)` | **Yes** — raw storage |
| `has_role(addr, "operator")` | No |

After `blacklist(addr)`:

| Function | Returns `"operator"` for `addr`? |
|---|---|
| `get_role_members("operator")` | No — filtered by `has_role_internal` |
| `get_roles_for_address(addr)` | **Yes** — raw storage |
| `has_role(addr, "operator")` | No |

---

## Impact

Any UI or off-chain tool that calls `get_roles_for_address` to display "roles held by
this address" will show roles that the address cannot exercise. This creates a misleading
picture of the access state and may cause operators to incorrectly trust that an address
has permissions it has lost.

---

## Required Change

Filter the result of `get_roles_for_address` before returning it, mirroring the approach
in `get_role_members`. Each role in `AddressRoles(addr)` must pass both:

1. The expiry check — the grant has not expired.
2. The blacklist check — `addr` is not blacklisted.

Both conditions are already captured by `has_direct_role_internal` (or the combination
of `has_role_internal` and `is_blacklisted_internal`). The fix avoids duplicating logic:

```rust
pub fn get_roles_for_address(env: Env, addr: Address) -> Vec<String> {
    router_common::extend_instance_ttl(&env, INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
    let all_roles: Vec<String> = env
        .storage()
        .instance()
        .get(&DataKey::AddressRoles(addr.clone()))
        .unwrap_or_else(|| Vec::new(&env));

    // Mirror get_role_members: exclude expired grants and blacklisted addresses.
    if Self::is_blacklisted_internal(&env, &addr) {
        return Vec::new(&env);
    }

    let mut active_roles = Vec::new(&env);
    for role in all_roles.iter() {
        if Self::has_direct_role_internal(&env, &addr, &role) {
            active_roles.push_back(role.clone());
        }
    }
    active_roles
}
```

If callers need the unfiltered list (e.g. for cleanup after bulk revocation), add a
separate function with an explicit name such as `get_all_stored_roles_for_address` that
documents its unfiltered nature.

---

## Tests Required

Two new tests in the `#[cfg(test)]` module of `contracts/router-access/src/lib.rs`:

**Expired grant is excluded:**
```rust
#[test]
fn test_get_roles_for_address_excludes_expired_role() {
    let (env, admin, client) = setup();
    let user = Address::generate(&env);
    // Grant a role with a past expiry.
    let past = env.ledger().timestamp() - 1;
    client.grant_role(&admin, &user, &String::from_str(&env, "operator"), &Some(past));
    // Advance ledger so the grant is expired.
    env.ledger().set_timestamp(env.ledger().timestamp() + 10);
    let roles = client.get_roles_for_address(&user);
    assert!(roles.is_empty());
}
```

**Blacklisted address returns empty:**
```rust
#[test]
fn test_get_roles_for_address_excludes_blacklisted_address() {
    let (env, admin, client) = setup();
    let user = Address::generate(&env);
    client.grant_role(&admin, &user, &String::from_str(&env, "operator"), &None);
    client.blacklist(&admin, &user);
    let roles = client.get_roles_for_address(&user);
    assert!(roles.is_empty());
}
```

---

## Acceptance Criteria

- [ ] `get_roles_for_address` excludes roles whose grant has expired.
- [ ] `get_roles_for_address` returns an empty list for a blacklisted address.
- [ ] `get_role_members` and `get_roles_for_address` are consistent: if `has_role(addr, role)` is `false`, neither function includes that role/address pair.
- [ ] Tests exist for both the expired-grant case and the blacklisted-address case.
- [ ] No existing tests are broken.
