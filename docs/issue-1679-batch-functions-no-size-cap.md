# Issue 1679 — Batch Functions Without a Size Cap

**Tracks:** [#1679](https://github.com/Maki-Zeninn/stellar-router/issues/1679)  
**Affects:** `contracts/router-core/src/lib.rs`,
`contracts/router-access/src/lib.rs`

---

## Problem

`router-registry` caps bulk operations at `MAX_BULK_BATCH_SIZE = 50` (added in
#1367), and `router-multicall` enforces `max_batch_size`. The following
batch functions in other contracts have no equivalent cap:

| Contract | Function | Concern |
|---|---|---|
| `router-core` | `register_routes_batch` | Runs out of transaction budget mid-batch |
| `router-core` | `remove_routes_batch` | Runs out of transaction budget mid-batch |
| `router-core` | `set_route_scores_batch` | Also recomputes best route after **every** entry |
| `router-access` | `grant_role_batch` | Runs out of transaction budget mid-batch |

All four functions are admin-only, so this is not an abuse vector — it is a
reliability issue. An oversized batch exceeds Soroban's transaction instruction
or memory budget partway through and fails with an opaque host error rather
than a clear, actionable error. For `set_route_scores_batch` the problem is
compounded: best-route recomputation happens on each update, so cost scales
super-linearly with batch size.

---

## Required Implementation Changes

### 1. Add `MAX_BATCH_SIZE` constants

In both contracts, add a module-level constant matching the registry's value:

```rust
/// Maximum number of entries accepted in a single batch call.
/// Matches `MAX_BULK_BATCH_SIZE` in router-registry (#1367).
/// Batches larger than this return `Error::BatchTooLarge`.
pub const MAX_BATCH_SIZE: u32 = 50;
```

### 2. Add `BatchTooLarge` error variant

Add `BatchTooLarge` to each contract's `Error` enum, using the next free
discriminant. For example (adjust the discriminant to the actual next value):

**router-core:**
```rust
#[contracterror]
pub enum Error {
    // ... existing variants ...
    BatchTooLarge = <next_free_discriminant>,
}
```

**router-access:**
```rust
#[contracterror]
pub enum Error {
    // ... existing variants ...
    BatchTooLarge = <next_free_discriminant>,
}
```

> Check `contracts/router-common/src/lib.rs` and each contract's existing
> error enum to find the correct next discriminant before assigning.

### 3. Guard each batch function

Add a length check at the top of each function, before any storage writes:

```rust
pub fn register_routes_batch(env: Env, caller: Address, routes: Vec<RouteInput>) -> Result<(), Error> {
    if routes.len() > MAX_BATCH_SIZE {
        return Err(Error::BatchTooLarge);
    }
    // ... existing implementation ...
}
```

Apply the same pattern to `remove_routes_batch`, `set_route_scores_batch`, and
`grant_role_batch`.

### 4. Defer best-route recomputation in `set_route_scores_batch`

`set_route_scores_batch` currently recomputes the best route after updating
each score. With a batch of N entries, this triggers N recomputations, each
touching all existing routes. Recompute once after all scores are written:

```rust
pub fn set_route_scores_batch(env: Env, caller: Address, scores: Vec<RouteScore>) -> Result<(), Error> {
    if scores.len() > MAX_BATCH_SIZE {
        return Err(Error::BatchTooLarge);
    }
    for entry in scores.iter() {
        write_score(&env, &entry.route, entry.score); // no recompute here
    }
    recompute_best_route(&env); // single recompute after all writes
    Ok(())
}
```

This reduces `set_route_scores_batch` complexity from O(N × R) to O(N + R)
where R is the number of registered routes.

---

## Required Documentation Changes

### `docs/error-codes.md`

Add entries for the new `BatchTooLarge` variants in the router-core and
router-access sections:

```markdown
### router-core

| Code | Name | Description |
|---|---|---|
| ... | ... | ... |
| <discriminant> | `BatchTooLarge` | Batch input exceeds `MAX_BATCH_SIZE` (50). Split the batch into smaller chunks. |

### router-access

| Code | Name | Description |
|---|---|---|
| ... | ... | ... |
| <discriminant> | `BatchTooLarge` | Batch input exceeds `MAX_BATCH_SIZE` (50). Split the batch into smaller chunks. |
```

### `docs/deployment.md` and usage docs

Wherever batch functions are documented or shown in examples, note the limit:

> Batch functions accept a maximum of **50 entries** per call. Larger inputs
> return `BatchTooLarge`. Split your data into chunks of 50 and call the
> function once per chunk.

---

## Required Test Cases

Each of the four functions needs two tests:

| Test | Expected result |
|---|---|
| Batch of exactly `MAX_BATCH_SIZE` (50) entries | Succeeds |
| Batch of `MAX_BATCH_SIZE + 1` (51) entries | Returns `Err(Error::BatchTooLarge)` |

Additionally, for `set_route_scores_batch`:

| Test | Expected result |
|---|---|
| Best-route value is correct after batch update | The best route reflects all updated scores (not just the last one) |
| `recompute_best_route` is called exactly once per batch call | Verified via storage-write count or event emission |

---

## Acceptance Criteria

- [ ] `register_routes_batch`, `remove_routes_batch`, `set_route_scores_batch`,
      and `grant_role_batch` all reject batches larger than `MAX_BATCH_SIZE`
      (50) with `Error::BatchTooLarge`
- [ ] `MAX_BATCH_SIZE = 50` is defined as a named constant in both contracts
- [ ] `BatchTooLarge` error variant is added with the correct next-free discriminant
- [ ] `set_route_scores_batch` recomputes best route once per call, not once per entry
- [ ] Tests exist for the exact limit (50) and one above the limit (51)
- [ ] `docs/error-codes.md` documents the new error variants
- [ ] Batch size limit is mentioned in relevant usage documentation
