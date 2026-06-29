# Contract Upgrade Guide

This document describes how to upgrade stellar-router contracts on Soroban,
what to consider for state migration, and the recommended process for each
contract in the suite.

## How Soroban Contract Upgrades Work

Soroban supports in-place WASM replacement via the host function
`update_current_contract_wasm`. When called, the contract's WASM bytecode is
replaced atomically. The contract's storage (all `DataKey` entries) is
**preserved** — the new WASM reads the same storage the old WASM wrote.

This means:
- Adding new storage keys is safe (old entries are simply absent until written).
- Removing storage keys is safe (old entries remain but are ignored).
- **Changing the type of an existing storage key is dangerous** — the new WASM
  will try to deserialize old data with the new type and will panic.
- Changing a `contracterror` discriminant value is a breaking change for
  callers that pattern-match on error codes.

---

## Upgrade Strategy

### Step 1 — Queue the upgrade via router-timelock

Never upgrade a contract directly. Always queue the upgrade as a timelock
operation so there is a delay window for review and cancellation.

```bash
stellar contract invoke --id <TIMELOCK_ID> --network testnet --source admin \
  -- queue \
  --proposer <ADMIN_ADDRESS> \
  --description "upgrade router-core to v2" \
  --target <CORE_CONTRACT_ID> \
  --delay 86400 \
  --depends_on "[]"
```

### Step 2 — Build the new WASM

```bash
cargo build --target wasm32-unknown-unknown --release
```

The new WASM will be at:
```
target/wasm32-unknown-unknown/release/router_core.wasm
```

### Step 3 — Upload the new WASM to the network

```bash
stellar contract upload \
  --wasm target/wasm32-unknown-unknown/release/router_core.wasm \
  --network testnet \
  --source admin
```

This returns a WASM hash. Note it — you will need it in step 4.

### Step 4 — Execute the upgrade after the delay

After the timelock delay has elapsed, execute the operation. The actual
`update_current_contract_wasm` call must be made from within the contract
itself (or via an authorized upgrade function). Add an `upgrade` function
to each contract:

```rust
pub fn upgrade(env: Env, caller: Address, new_wasm_hash: soroban_sdk::BytesN<32>) -> Result<(), RouterError> {
    caller.require_auth();
    Self::require_admin(&env, &caller)?;
    env.deployer().update_current_contract_wasm(new_wasm_hash);
    Ok(())
}
```

Then invoke it:

```bash
stellar contract invoke --id <CORE_ID> --network testnet --source admin \
  -- upgrade \
  --caller <ADMIN_ADDRESS> \
  --new_wasm_hash <WASM_HASH_FROM_STEP_3>
```

---

## State Migration

### Safe changes (no migration needed)

| Change | Safe? | Notes |
|---|---|---|
| Add a new `pub fn` | ✅ | New function, no storage impact |
| Add a new `DataKey` variant | ✅ | Old storage unaffected |
| Add a new `contracterror` variant | ✅ | New discriminant, old callers unaffected |
| Add a field to a struct (with default) | ⚠️ | Only safe if old data can be deserialized — Soroban uses XDR, which is not forward-compatible by default |
| Remove an unused `pub fn` | ✅ | No storage impact |

### Dangerous changes (migration required)

| Change | Risk | Mitigation |
|---|---|---|
| Change the type of an existing `DataKey` value | 🔴 Panic on read | Migrate data before upgrading (see below) |
| Change a `contracterror` discriminant number | 🔴 Breaking for callers | Never reuse discriminant numbers; only add new ones |
| Remove a `DataKey` variant that is still in storage | 🟡 Orphaned data | Acceptable if the data is no longer needed; document it |
| Rename a `contracttype` struct field | 🔴 XDR deserialization failure | Add a new struct, migrate data, remove old struct in a follow-up upgrade |

### Migration pattern

If you need to change a storage type, use a two-phase upgrade:

**Phase 1 — migration upgrade:**
1. Add the new `DataKey` variant (e.g., `RouteEntryV2`).
2. Add a `migrate()` function that reads all `RouteEntry` values, converts them
   to `RouteEntryV2`, writes them under the new key, and removes the old key.
3. Deploy this upgrade.
4. Call `migrate()` once.

**Phase 2 — cleanup upgrade:**
1. Remove the old `DataKey::RouteEntry` variant and all code that references it.
2. Deploy this upgrade.

---

## Per-Contract Upgrade Notes

### router-core

- `RouteEntry` struct has an `Option<RouteMetadata>` field. Adding fields to
  `RouteMetadata` requires a migration if existing entries are stored.
- `DataKey::RouteNames` and `DataKey::Aliases` are `Vec<String>` — safe to
  extend but not to change the element type.
- The `admin()` function panics if the contract is not initialized. Ensure
  `initialize()` has been called before upgrading.

### router-registry

- `ContractEntry` stores `registered_by: Address`. Adding a `registered_at: u64`
  timestamp field requires a migration for existing entries.
- Version lists (`DataKey::Versions`) are `Vec<u32>` — safe to extend.

### router-access

- `DataKey::RoleParent` is new in the hierarchy feature. Old deployments without
  it will simply have no parent relationships — safe to add without migration.
- `DataKey::HasRole` stores `bool`. Do not change this to a struct without a
  migration.

### router-middleware

- `RouteConfig` has grown over time (added `failure_threshold`,
  `recovery_window_seconds`, `log_retention`). If upgrading from an older
  deployment, existing `RouteConfig` entries will fail to deserialize with the
  new struct. Run a migration that re-writes all `RouteConfig` entries with
  default values for the new fields.

### router-timelock

- `TimelockOp` has `is_critical: bool`. Old entries without this field will
  fail to deserialize. If upgrading from a pre-hierarchy deployment, migrate
  all existing operations to set `is_critical = false`.
- `DataKey::FastTrackEnabled` is new — safe to add without migration.

### router-multicall

- `CallDescriptor` has `instruction_budget: Option<u64>`. Old entries without
  this field will fail to deserialize if stored. Since `execute_batch` does not
  persist `CallDescriptor` values, this is safe.

---

## Zero-Downtime Upgrade Path

In-place WASM replacement (described above) is the preferred path because it
preserves all storage and requires no state migration. Use it whenever your
changes are backward-compatible (new functions, new `DataKey` variants, bug
fixes that do not change stored types).

When a breaking storage change is unavoidable, you must deploy a **new contract
instance** and migrate state. This is more disruptive but can still be done with
minimal downtime:

### Phase A — Deploy and populate the new contract

1. Deploy a fresh contract instance with the new WASM:

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/router_core.wasm \
  --network mainnet \
  --source admin
```

Save the new `<NEW_CONTRACT_ID>`.

2. Initialize it:

```bash
stellar contract invoke --id <NEW_CONTRACT_ID> --network mainnet --source admin \
  -- initialize --admin <ADMIN_ADDRESS>
```

3. Run your off-chain migration script to copy all routes, aliases, scores, and
   metadata from the old contract to the new one. Keep the old contract alive
   and accepting reads during this window.

Example migration script using the Stellar CLI:

```bash
#!/usr/bin/env bash
set -euo pipefail

OLD_ID="<OLD_CONTRACT_ID>"
NEW_ID="<NEW_CONTRACT_ID>"
NETWORK="mainnet"
SOURCE="admin"

# Fetch all route names from the old contract
NAMES=$(stellar contract invoke --id "$OLD_ID" --network "$NETWORK" --source "$SOURCE" \
  -- get_route_names 2>/dev/null | jq -r '.[]')

for name in $NAMES; do
  # Read route entry
  ADDR=$(stellar contract invoke --id "$OLD_ID" --network "$NETWORK" --source "$SOURCE" \
    -- resolve --name "$name" 2>/dev/null)

  # Register on new contract
  stellar contract invoke --id "$NEW_ID" --network "$NETWORK" --source "$SOURCE" \
    -- register --caller "$ADMIN_ADDRESS" --name "$name" --address "$ADDR"

  echo "Migrated route: $name -> $ADDR"
done

# Fetch and migrate aliases
ALIASES=$(stellar contract invoke --id "$OLD_ID" --network "$NETWORK" --source "$SOURCE" \
  -- get_aliases 2>/dev/null | jq -r '.[]')

for alias in $ALIASES; do
  TARGET=$(stellar contract invoke --id "$OLD_ID" --network "$NETWORK" --source "$SOURCE" \
    -- get_alias --alias_name "$alias" 2>/dev/null)

  stellar contract invoke --id "$NEW_ID" --network "$NETWORK" --source "$SOURCE" \
    -- add_alias --caller "$ADMIN_ADDRESS" --existing_name "$TARGET" --alias_name "$alias"

  echo "Migrated alias: $alias -> $TARGET"
done

echo "Migration complete."
```

### Phase B — Atomic cutover

Once the new contract is fully populated:

1. Point your off-chain registry and any router-registry entries to
   `<NEW_CONTRACT_ID>`.
2. Pause the old contract to prevent stale writes:

```bash
stellar contract invoke --id <OLD_CONTRACT_ID> --network mainnet --source admin \
  -- set_paused --caller <ADMIN_ADDRESS> --paused true
```

3. Verify the new contract is serving traffic correctly (see verification steps
   below).

### Phase C — Decommission the old contract

After a monitoring window (at least 24 hours), you may stop routing traffic to
the old contract. Because Soroban contracts cannot be deleted, simply cease
calling it. Update all documentation and tooling to reference `<NEW_CONTRACT_ID>`
only.

---

## Pre/Post-Upgrade Verification

### Before upgrading

Confirm the current state is readable and the contract responds correctly:

```bash
# Verify admin is set
stellar contract invoke --id <CONTRACT_ID> --network mainnet --source admin \
  -- admin

# Verify route count is non-zero
stellar contract invoke --id <CONTRACT_ID> --network mainnet --source admin \
  -- get_route_count

# Spot-check a known route
stellar contract invoke --id <CONTRACT_ID> --network mainnet --source admin \
  -- resolve --name <KNOWN_ROUTE_NAME>
```

### After upgrading

Run the same checks immediately after the upgrade executes:

```bash
# Same three checks above — if any fail, initiate rollback immediately

# Additionally verify the new WASM hash matches what was uploaded
stellar contract info --id <CONTRACT_ID> --network mainnet | grep wasm_hash
```

---

## Rollback

Soroban does not support automatic rollback of a WASM upgrade. If an upgrade
introduces a bug:

1. Build the previous WASM version.
2. Upload it to the network:

```bash
stellar contract upload \
  --wasm target/wasm32-unknown-unknown/release/router_core_previous.wasm \
  --network mainnet \
  --source admin
```

3. Call `upgrade()` with the old WASM hash:

```bash
stellar contract invoke --id <CONTRACT_ID> --network mainnet --source admin \
  -- upgrade --caller <ADMIN_ADDRESS> --new_wasm_hash <PREVIOUS_WASM_HASH>
```

**Always upload the rollback WASM before the upgrade executes** (step 3 of the
upgrade strategy) so the hash is available immediately if something goes wrong.

This is why all upgrades should be queued through router-timelock — the delay
window gives time to test the new WASM on testnet and cancel the upgrade if
issues are found before it executes on mainnet.

---

## Upgrade Checklist

### General (all contracts)

Before upgrading any contract on mainnet:

- [ ] New WASM tested on testnet with production-like data
- [ ] Storage compatibility verified (no type changes without migration)
- [ ] `contracterror` discriminants unchanged
- [ ] Upgrade queued via router-timelock with at least 24h delay
- [ ] Migration function (if needed) tested on testnet
- [ ] Rollback WASM uploaded and hash noted
- [ ] Pre-upgrade verification commands pass (see Pre/Post-Upgrade Verification)
- [ ] On-chain monitoring active during upgrade window
- [ ] Post-upgrade verification commands pass; rollback initiated immediately if any fail

### router-core

- [ ] Confirmed no changes to `RouteEntry` field types (XDR incompatibility)
- [ ] Confirmed `DataKey` variants not reordered
- [ ] All aliases resolve correctly after upgrade
- [ ] Scored routes return the expected best-route after upgrade

### router-registry

- [ ] `ContractEntry` fields unchanged or two-phase migration prepared
- [ ] Version lists (`DataKey::Versions`) remain `Vec<u32>`

### router-access

- [ ] Role hierarchy entries still readable (check `DataKey::RoleParent`)
- [ ] `DataKey::HasRole` type unchanged

### router-middleware

- [ ] `RouteConfig` struct fields not removed or reordered
- [ ] Circuit breaker state (`DataKey::CircuitState`) readable after upgrade

### router-timelock

- [ ] `TimelockOp.is_critical` field still present
- [ ] No pending timelock operations that would be invalidated by the schema change
- [ ] Cancel any in-flight operations before a schema-breaking upgrade

### router-multicall

- [ ] `CallDescriptor` changes are safe (it is not persisted, so field additions are safe)

---

## Registry Version Coordination

`router-registry` tracks deployed contract versions in `DataKey::Versions`
(a `Vec<u32>`). When you deploy a new WASM version via in-place upgrade, register
the new version in the registry immediately after the upgrade executes:

```bash
stellar contract invoke --id <REGISTRY_ID> --network mainnet --source admin \
  -- register_version \
  --caller <ADMIN_ADDRESS> \
  --contract_id <CORE_CONTRACT_ID> \
  --version <NEW_VERSION_NUMBER>
```

When you deploy a completely new contract instance (state-migration path), register
it as a new entry rather than a new version of the old contract:

```bash
stellar contract invoke --id <REGISTRY_ID> --network mainnet --source admin \
  -- register \
  --caller <ADMIN_ADDRESS> \
  --name "router-core-v2" \
  --address <NEW_CONTRACT_ID>
```

Keep both the old and new entries in the registry until the old contract is fully
decommissioned, so tooling and dashboards can reference either address by name.

---

## Storage Schema Migration Guide

This section covers how to handle storage schema changes when upgrading deployed
contracts. Because Soroban preserves all `DataKey` entries across WASM upgrades,
changing the layout of stored values can make existing data unreadable.

### Storage Schema Versioning

Each contract's `DataKey` enum is the schema. Treat it the same way you would a
database schema: only append new variants, never change or reorder existing ones.

- **Adding a new `DataKey` variant** — safe. Old storage is unaffected; the new
  key simply starts absent.
- **Removing a `DataKey` variant** — safe if no live entries exist for it.
  Orphaned entries remain in storage but are ignored.
- **Changing the payload type of an existing variant** — dangerous. The new WASM
  will attempt to deserialize old bytes with the new type and will panic.
- **Reordering `DataKey` variants** — dangerous. Soroban encodes `contracttype`
  enums by discriminant index, so reordering changes which numeric value maps to
  which key name.

### Storage Compatibility Matrix

| Change | Safe? | Migration Needed? |
|---|---|---|
| Add new `DataKey` variant | ✅ Yes | No |
| Remove unused `DataKey` variant | ✅ Yes | No (orphaned data stays but is ignored) |
| Add optional field to a stored struct | ⚠️ Risky | Yes — XDR is not forward-compatible; use two-phase migration |
| Change variant payload type | ❌ No | Yes — data will fail to deserialize |
| Reorder `DataKey` variants | ❌ No | Yes — discriminants shift, wrong keys are read |
| Change a `contracterror` discriminant number | ❌ No | Callers that pattern-match on error codes will misinterpret |

### Migration Patterns

#### 1. Lazy migration

Check the schema version on each read and migrate the entry on first access.
Useful when entries are read infrequently or the data set is large.

```rust
pub fn get_route_entry(env: &Env, name: &String) -> RouteEntry {
    // Try the new key first
    if let Some(entry) = env.storage().instance().get::<DataKey, RouteEntryV2>(&DataKey::RouteV2(name.clone())) {
        return entry;
    }
    // Fall back to old key and migrate
    let old: RouteEntryV1 = env.storage().instance()
        .get(&DataKey::Route(name.clone()))
        .expect("route not found");
    let new_entry = RouteEntryV2 { address: old.address, name: old.name, paused: old.paused, updated_by: old.updated_by, version: 0 };
    env.storage().instance().set(&DataKey::RouteV2(name.clone()), &new_entry);
    env.storage().instance().remove(&DataKey::Route(name.clone()));
    new_entry
}
```

#### 2. Batch migration

Add an admin-only `migrate()` function that iterates all entries and rewrites
them under the new key. Call it once after deploying the new WASM.

```rust
pub fn migrate(env: Env, caller: Address) -> Result<u32, RouterError> {
    caller.require_auth();
    Self::require_admin(&env, &caller)?;

    let names = Self::get_route_names(&env);
    let mut count = 0u32;
    for name in names.iter() {
        if let Some(old) = env.storage().instance().get::<DataKey, RouteEntryV1>(&DataKey::Route(name.clone())) {
            let new_entry = RouteEntryV2 { address: old.address, name: old.name, paused: old.paused, updated_by: old.updated_by, version: 0 };
            env.storage().instance().set(&DataKey::RouteV2(name.clone()), &new_entry);
            env.storage().instance().remove(&DataKey::Route(name.clone()));
            count += 1;
        }
    }
    Ok(count)
}
```

#### 3. Dual-read

Support old and new schema simultaneously during a transition period. The new
WASM reads from both keys and writes only to the new one. After all entries are
migrated, deploy a follow-up upgrade that removes the old key support.

```rust
fn get_entry(env: &Env, name: &String) -> Option<RouteEntryV2> {
    // New key first
    if let Some(e) = env.storage().instance().get(&DataKey::RouteV2(name.clone())) {
        return Some(e);
    }
    // Old key fallback (read-only, no migration)
    env.storage().instance()
        .get::<DataKey, RouteEntryV1>(&DataKey::Route(name.clone()))
        .map(|old| RouteEntryV2 { address: old.address, name: old.name, paused: old.paused, updated_by: old.updated_by, version: 0 })
}
```

### Two-Phase Migration Procedure

When a stored struct needs a new field (e.g., adding `registered_at: u64` to
`ContractEntry`):

**Phase 1 — migration upgrade:**
1. Add `DataKey::ContractEntryV2` alongside the existing `DataKey::ContractEntry`.
2. Add `ContractEntryV2` struct with the new field.
3. Add a `migrate()` admin function that reads all `ContractEntry` values,
   writes them as `ContractEntryV2` (setting the new field to a sensible default),
   and removes the old key.
4. Deploy this upgrade and call `migrate()`.

**Phase 2 — cleanup upgrade:**
1. Remove `DataKey::ContractEntry`, `ContractEntryV1`, and all code that
   references them.
2. Rename `ContractEntryV2` back to `ContractEntry` in a new deployment (this
   is safe because the in-storage discriminant for `ContractEntryV2` is now the
   only live key).
3. Deploy this upgrade.

### Migration Checklist

Before any upgrade that changes a stored type:

- [ ] Verify all existing storage keys are readable after the upgrade (test with
  populated storage from the previous version on testnet)
- [ ] Write and run a migration function against testnet state
- [ ] Document the rollback procedure if migration fails mid-run
- [ ] Keep old `DataKey` variants in source until all live entries are migrated
- [ ] Queue the migration upgrade via router-timelock for a 24h review window
