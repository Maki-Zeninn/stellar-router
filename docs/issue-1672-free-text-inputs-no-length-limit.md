# Issue #1672 — Deprecation Reasons and Operation Descriptions Have No Length Limit

**Tracking:** [#1672](https://github.com/Maki-Zeninn/stellar-router/issues/1672)  
**Component:** `contracts/router-registry`, `contracts/router-timelock`  
**Type:** Security / Resource  
**Impact:** Unbounded free-text strings in instance storage inflate every future read and can push the instance entry toward the network's size limit

---

## Summary

Two contracts accept free-text `String` inputs and store them in instance storage
without any size check:

- **`router-registry`** — the `reason: Option<String>` parameter in `deprecate`,
  `deprecate_all_versions`, and `deprecate_many` is written directly into
  `ContractEntry.deprecation_reason`. `deprecate_all_versions` copies the same
  string into every version's entry, multiplying the effect.
- **`router-timelock`** — `queue` stores `description` in `Op.description` and
  hashes the full string into the `op_id`. `update_description` replaces the stored
  value with no limit.

Because all state lives in instance storage (the same entry that every other operation
reads), one multi-kilobyte string increases the cost of every subsequent read on that
contract. A handful of such writes can push the instance entry toward the per-entry
size limit enforced by the network.

`router-core` already applies `MAX_METADATA_DESCRIPTION_LEN = 256` to `RouteMetadata`
via `validate_metadata`. The same discipline should apply here.

---

## Affected Locations

### `contracts/router-registry/src/lib.rs`

```rust
// ContractEntry definition — reason stored without cap
pub struct ContractEntry {
    pub deprecated: bool,
    pub deprecation_reason: Option<String>,  // ← no length limit
    // ...
}

// deprecate — passes reason straight to deprecate_one
pub fn deprecate(
    env: Env, caller: Address, name: String, version: u32, reason: Option<String>,
) -> Result<(), RegistryError> {
    // ...
    Self::deprecate_one(&env, name, version, reason)   // ← no validation before this
}

// deprecate_all_versions — copies the same unbounded reason into every version
pub fn deprecate_all_versions(
    env: Env, caller: Address, name: String, reason: Option<String>,
) -> Result<(), RegistryError> {
    // ...
    for v in versions.iter() {
        let _ = Self::deprecate_one(&env, name.clone(), v, reason.clone());
    }
}

// deprecate_many — same issue, iterated over multiple (name, version) pairs
pub fn deprecate_many(...) { ... }
```

### `contracts/router-timelock/src/lib.rs`

```rust
pub struct Op {
    pub description: String,   // ← no length limit
    // ...
}

// queue — hashes full description into op_id, then stores it
pub fn queue(
    env: Env, caller: Address, description: String, ...
) -> Result<Bytes, TimelockError> {
    // ...
    preimage.append(&description.clone().to_xdr(&env));  // ← full string in hash preimage
    // ...
    env.storage().instance().set(&DataKey::Op(op_id.clone()), &op);  // ← stored without cap
}

// update_description — replaces stored description with no limit
pub fn update_description(
    env: Env, caller: Address, op_id: Bytes, new_description: String,
) -> Result<(), TimelockError> {
    // ...
    op.description = new_description.clone();   // ← no validation
}
```

---

## Comparison: `router-core` Already Does This Correctly

```rust
// contracts/router-core/src/lib.rs
pub const MAX_METADATA_DESCRIPTION_LEN: u32 = 256;

fn validate_metadata(meta: &RouteMetadata) -> Result<(), RouterError> {
    if meta.description.len() > MAX_METADATA_DESCRIPTION_LEN || ... {
        return Err(RouterError::MetadataTooLong);
    }
    // ...
}
```

---

## Required Changes

### `router-registry`

1. Add a constant:
   ```rust
   pub const MAX_DEPRECATION_REASON_LEN: u32 = 256;
   ```

2. Add a new error variant:
   ```rust
   pub enum RegistryError {
       // ...
       DeprecationReasonTooLong,
   }
   ```

3. Validate at the top of `deprecate_one` (the single helper all three public functions
   call), before writing:
   ```rust
   fn deprecate_one(
       env: &Env, name: String, version: u32, reason: Option<String>,
   ) -> Result<(), RegistryError> {
       if let Some(ref r) = reason {
           if r.len() > MAX_DEPRECATION_REASON_LEN {
               return Err(RegistryError::DeprecationReasonTooLong);
           }
       }
       // ... rest of existing logic
   }
   ```

   Validating in `deprecate_one` covers all three callers (`deprecate`,
   `deprecate_all_versions`, `deprecate_many`) without touching each one separately.

### `router-timelock`

1. Add a constant:
   ```rust
   pub const MAX_DESCRIPTION_LEN: u32 = 256;
   ```

2. Add a new error variant:
   ```rust
   pub enum TimelockError {
       // ...
       DescriptionTooLong,
   }
   ```

3. Validate at the start of `queue` and `update_description`:
   ```rust
   pub fn queue(env: Env, caller: Address, description: String, ...) -> Result<Bytes, TimelockError> {
       if description.len() > MAX_DESCRIPTION_LEN {
           return Err(TimelockError::DescriptionTooLong);
       }
       // ... existing logic
   }

   pub fn update_description(
       env: Env, caller: Address, op_id: Bytes, new_description: String,
   ) -> Result<(), TimelockError> {
       if new_description.len() > MAX_DESCRIPTION_LEN {
           return Err(TimelockError::DescriptionTooLong);
       }
       // ... existing logic
   }
   ```

---

## Tests Required

For each validation point, add two tests: one at the limit (must succeed) and one at
limit + 1 (must fail).

**`router-registry` — `deprecate`:**
```rust
#[test]
fn test_deprecate_reason_at_limit_succeeds() {
    // reason.len() == MAX_DEPRECATION_REASON_LEN → Ok
}

#[test]
fn test_deprecate_reason_over_limit_fails() {
    // reason.len() == MAX_DEPRECATION_REASON_LEN + 1 → Err(DeprecationReasonTooLong)
}
```

Same pair for `deprecate_all_versions` and `deprecate_many`.

**`router-timelock` — `queue`:**
```rust
#[test]
fn test_queue_description_at_limit_succeeds() { ... }

#[test]
fn test_queue_description_over_limit_fails() { ... }
```

**`router-timelock` — `update_description`:**
```rust
#[test]
fn test_update_description_at_limit_succeeds() { ... }

#[test]
fn test_update_description_over_limit_fails() { ... }
```

---

## Acceptance Criteria

- [ ] `router-registry` defines `MAX_DEPRECATION_REASON_LEN` (e.g. 256).
- [ ] `router-registry` rejects reasons exceeding the limit with a dedicated error variant.
- [ ] `router-timelock` defines `MAX_DESCRIPTION_LEN` (e.g. 256).
- [ ] `router-timelock` rejects descriptions exceeding the limit in both `queue` and
  `update_description` with a dedicated error variant.
- [ ] Tests exist at the limit and at limit + 1 for each validation point.
- [ ] No existing tests are broken.
