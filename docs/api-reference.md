# API Reference

Complete reference for all public functions across the stellar-router contracts: core, registry, access, quote, middleware, timelock, multicall and execution.

---

## Error Code Reference

Soroban contract errors are emitted as numeric enum discriminants. Error names
and values are scoped to each contract, so the same numeric value can mean
different things depending on which contract returned it.

### router-core `RouterError`

| Error | Value | When it occurs | How to handle it |
| --- | ---: | --- | --- |
| `AlreadyInitialized` | 1 | `initialize` is called after the router has already been initialized. | Treat initialization as complete, or deploy a fresh contract if you need a new admin. |
| `NotInitialized` | 2 | Admin-only helpers are called before `initialize` has stored the admin. | Initialize the router before calling configuration or admin reads. |
| `Unauthorized` | 3 | The caller is not the stored router admin. | Sign with the admin account or transfer admin first. |
| `RouteNotFound` | 4 | A route or alias lookup targets a name that is not registered. | Register the route, check spelling, or remove stale aliases in the caller. |
| `RoutePaused` | 5 | `resolve` targets a route that has been individually paused. | Unpause the route or route traffic elsewhere. |
| `RouterPaused` | 6 | `resolve` is called while the global router pause is active. | Wait for recovery or have the admin call `set_paused(false)`. |
| `RouteAlreadyExists` | 7 | Registering a route or alias with a name that is already in use. | Choose a unique name or update/remove the existing route first. |
| `InvalidRouteName` | 8 | A route name is empty or only whitespace. | Send a non-empty, trimmed route name. |
| `InvalidMetadata` | 9 | Route metadata exceeds limits: description over 256 characters or more than 5 tags. | Shorten the description or reduce the tag list before submitting. |
| `CircularDependency` | 10 | `set_route_dependency` would create a self-dependency or cycle, or `resolve_with_dependencies` finds a cycle. | Remove or restructure the dependency edge. |
| `RouteInUse` | 11 | `remove_route` (or a `remove_routes_batch` item) targets a route that other routes depend on. | Remove the dependent routes or their dependency edges first. |
| `InvalidAddress` | 12 | A route is registered or updated with the zero address. | Pass a real contract address. |
| `RouteExpired` | 13 | `resolve` targets a route whose TTL ledger has been exceeded, or `extend_route_ttl` targets a route that has already expired. | Re-register the route, or call `extend_route_ttl` before expiry next time. |
| `InvalidScore` | 14 | A `RouteScore` field is above 100 in `set_route_score` / `set_route_scores_batch`. | Keep every score field in `0..=100`. |
| `InvalidTtlExtension` | 15 | `register_route_with_ttl` gets `ttl_ledgers = Some(0)` or `extend_route_ttl` gets `additional_ledgers = 0`. | Pass a positive ledger count, or `None` for a permanent route. |
| `RecursionLimitExceeded` | 16 | The dependency graph is deeper than 10 levels while validating or resolving dependencies. | Flatten the dependency graph. |
| `TooManyRoutes` | 17 | Registering a route would exceed `get_max_routes()`. | Remove routes or raise the cap with `set_max_routes`. |
| `InvalidScoringWeights` | 18 | `set_scoring_weights` gets `fee_divisor <= 0`, a negative weight, or a weight above 1000. | Use weights in `0..=1000` and a positive `fee_divisor`. |

### router-registry `RegistryError`

| Error | Value | When it occurs | How to handle it |
| --- | ---: | --- | --- |
| `AlreadyInitialized` | 1 | `initialize` is called after the registry has already been initialized. | Skip initialization or deploy a fresh registry for a new admin. |
| `NotInitialized` | 2 | Admin-dependent registry operations run before initialization. | Initialize the registry before writes or admin reads. |
| `Unauthorized` | 3 | The caller is not the registry admin. | Sign with the admin account or transfer admin first. |
| `NotFound` | 4 | No entry exists for the requested name/version, or no version satisfies a lookup. | Register the contract or adjust the requested name, version, or constraint. |
| `AlreadyRegistered` | 5 | The same `(name, version)` pair is registered twice. | Use a new version or deprecate old entries instead of overwriting. |
| `AlreadyDeprecated` | 6 | `deprecate` targets an entry that is already deprecated. | Treat the operation as already complete or choose another version. |
| `InvalidVersion` | 7 | Version is `0` or not greater than the current highest version for the name. | Use a positive version greater than existing versions. |
| `VersionNotFound` | 8 | `deprecate` targets a version that is not registered for the name. | Check `versions(name)` and retry with an existing version. |
| `InvalidConstraint` | 9 | A semver constraint cannot be parsed or uses an unsupported form. | Use exact, `>`, `>=`, `<`, `<=`, `^`, or `~` constraints. |
| `AllVersionsDeprecated` | 10 | Lookup finds versions, but every matching entry is deprecated. | Register a newer active version or allow deprecated versions in the caller logic. |
| `ContractUnreachable` | 11 | `register_with_check` could not successfully invoke the supplied health function on the target address. | Confirm the target exposes the allow-listed `health` or `ping` function and that it returns successfully. |
| `InvalidHealthFn` | 12 | `register_with_check` was given a `health_fn` outside the `health` / `ping` allow-list. | Pass `None` or use `health` / `ping`. |
| `BatchTooLarge` | 13 | `bulk_register` or `deprecate_many` was called with more than 50 entries (`MAX_BULK_BATCH_SIZE`). | Split the entries into batches of at most 50. |

### router-access `AccessError`

| Error | Value | When it occurs | How to handle it |
| --- | ---: | --- | --- |
| `AlreadyInitialized` | 1 | `initialize` is called after the access contract has already been initialized. | Skip initialization or deploy a fresh access contract. |
| `NotInitialized` | 2 | Admin or role checks require the super-admin before it is stored. | Initialize the contract with a super-admin first. |
| `Unauthorized` | 3 | The caller lacks super-admin or role-admin authority for the action. | Use an authorized signer or grant the required admin role first. |
| `AlreadyHasRole` | 4 | `grant_role` targets an address that already holds the role directly. | Treat as success, wait for expiry, or revoke before granting again. |
| `RoleNotFound` | 5 | `revoke_role` targets an address without that direct role grant. | Check `has_role` or role member lists before revoking. |
| `Blacklisted` | 6 | A blacklisted address is granted or checked for role access. | Unblacklist the address before granting roles. |
| `CannotBlacklistAdmin` | 7 | The caller tries to blacklist the super-admin address. | Transfer super-admin first, or choose another target. |
| `DestinationAlreadyHasRole` | 8 | `transfer_role_membership` targets a destination that already holds the role actively. | Revoke the destination's role first, or skip the transfer. |
| `HierarchyCycle` | 9 | `set_role_parent` would create a parent-role cycle. | Pick a parent role whose ancestor chain does not lead back to the role. |
| `InvalidExpiry` | 10 | `expires_in` added to the current ledger timestamp overflows `u64` when granting a role. | Use a smaller `expires_in`, or `None` for a non-expiring grant. |
| `HierarchyTooDeep` | 11 | `set_role_parent` would make the parent chain exceed the maximum hierarchy depth. | Flatten the role hierarchy. |
| `MaxRolesExceeded` | 12 | A new role name would exceed the configured `max_roles` cap. | Reuse an existing role or raise the cap via `set_role_limits`. |
| `MaxGrantsPerRoleExceeded` | 13 | Granting a role to a new address would exceed the configured `max_grants_per_role` cap. | Revoke unused grants or raise the cap via `set_role_limits`. |

### router-middleware `MiddlewareError`

| Error | Value | When it occurs | How to handle it |
| --- | ---: | --- | --- |
| `AlreadyInitialized` | 1 | `initialize` is called after middleware has already been initialized. | Skip initialization or deploy a fresh middleware contract. |
| `NotInitialized` | 2 | Middleware configuration or admin reads run before initialization. | Initialize the contract before route configuration. |
| `Unauthorized` | 3 | The caller is not the middleware admin. | Sign with the admin account or transfer admin first. |
| `RateLimitExceeded` | 4 | A caller has used all allowed calls for the route's current window. | Retry after the window resets or raise the configured limit. |
| `RouteDisabled` | 5 | `pre_call` runs against a route whose middleware config is disabled. | Enable the route or bypass it intentionally. |
| `MiddlewareDisabled` | 6 | Global middleware enforcement is disabled. | Re-enable middleware or treat the route as temporarily unavailable. |
| `InvalidConfig` | 7 | `configure_route` is called with `max_calls_per_window > 0` but `window_seconds == 0`, or with `log_retention > 10000`; or `configure_caller_rate_limit` is called with `max_calls > 0` but `window_secs == 0`. | Set a positive window (or `0` max calls for unlimited), and keep `log_retention` at or below `10000`. |
| `CircuitOpen` | 8 | The route's circuit breaker is open after repeated failures. | Wait for the recovery window, reset the breaker, or fix the failing downstream route. |
| _(retired)_ | 9 | Formerly `Reentrancy`. Never returned: the `pre_call` reentrancy guard was removed (Soroban rejects contract re-entry at the host level). The value is reserved so it is not confused with errors from old deployments. | — |

### router-timelock `TimelockError`

| Error | Value | When it occurs | How to handle it |
| --- | ---: | --- | --- |
| `AlreadyInitialized` | 1 | `initialize` is called after the timelock has already been initialized. | Skip initialization or deploy a fresh timelock contract. |
| `NotInitialized` | 2 | Timelock operations need the admin or delay before initialization. | Initialize the timelock with an admin and minimum delay. |
| `Unauthorized` | 3 | The caller is not the timelock admin. | Sign with the admin account or transfer admin first. |
| `NotFound` | 4 | `execute` or `cancel` targets an unknown operation id. | Recompute the operation id or query the operation before acting. |
| `NotReady` | 5 | `execute` is called before the queued operation's ETA. | Wait until the ETA has passed. |
| `AlreadyExecuted` | 6 | `execute` or `cancel` targets an operation that has already executed. | Treat it as complete and avoid replaying the operation. |
| `Cancelled` | 7 | `execute` or `cancel` targets an operation that has been cancelled. | Queue a new operation if the action is still needed. |
| `DelayTooShort` | 8 | `queue` uses a delay lower than the configured minimum delay, or `initialize` / `set_min_delay` is given a minimum delay of zero. | Use at least `min_delay()` seconds; pass a non-zero minimum delay. |
| `Expired` | 9 | The grace period has elapsed; the operation can no longer be executed. | Queue a new operation if the action is still needed. |
| `QueueFull` | 10 | The maximum number of pending operations has been reached. | Execute, cancel, or clean up pending operations before queueing more. |
| `CircularDependency` | 11 | A dependency references itself or creates a cycle. | Remove the self-referencing or cyclic dependency. |
| `DependencyTooDeep` | 12 | Dependency chain exceeds maximum allowed depth. | Flatten the dependency chain. |
| `AlreadyQueued` | 13 | An operation with this exact (description, target, eta) already exists. | Change the description, target, or delay, or use the existing operation. |
| `DependencyNotExecuted` | 14 | A dependency of this operation has not yet been executed. | Execute the dependencies first, then retry. |
| `GracePeriodTooLong` | 15 | `grace_period_seconds` exceeds `MAX_GRACE_PERIOD_SECONDS`. | Use a shorter grace period. |
| `ExpiryOverflow` | 16 | `eta + grace_period_seconds` overflows `u64`. | Should not occur after the `GracePeriodTooLong` guard; queue a new operation. |
| `DelayTooLong` | 17 | `ledger_timestamp + delay` overflows `u64`. | Use a smaller delay. |

### router-multicall `MulticallError`

| Error | Value | When it occurs | How to handle it |
| --- | ---: | --- | --- |
| `AlreadyInitialized` | 1 | `initialize` is called after multicall has already been initialized. | Skip initialization or deploy a fresh multicall contract. |
| `NotInitialized` | 2 | Batch execution or admin reads require config before initialization. | Initialize the contract with an admin and batch size. |
| `Unauthorized` | 3 | The caller is not the multicall admin for configuration changes. | Sign with the admin account or transfer admin first. |
| `BatchTooLarge` | 4 | `execute_batch` receives more calls than `max_batch_size`. | Split the batch or increase `max_batch_size`. |
| `EmptyBatch` | 5 | `execute_batch` is called with no calls. | Provide at least one call descriptor. |
| `RequiredCallFailed` | 6 | A required call in the batch fails. | Inspect per-call results, fix the failed required call, or mark it optional if safe. |
| `InvalidConfig` | 7 | `max_batch_size` is set to `0`. | Configure a positive maximum batch size. |

---

## router-core

**Contract:** `RouterCore`  
**Purpose:** Central dispatcher — registers routes by name and resolves them to contract addresses.

### `initialize(admin: Address, max_routes: Option<u32>) → Result<(), RouterError>`
Sets up the admin, marks the router as unpaused, resets the total-routed counter, and sets the route cap (`max_routes`, default `1000`). `admin` must authorize.  
Must be called exactly once before any other function.

**Errors:** `AlreadyInitialized`, `InvalidRouteName` (`max_routes` is `Some(0)`)

```bash
stellar contract invoke --id <CORE_ID> --network testnet --source admin \
  -- initialize --admin <ADMIN_ADDRESS>
```

---

### `register_route(caller, name, address, metadata: Option<RouteMetadata>) → Result<(), RouterError>`
Registers a new route, optionally with metadata (see [Tags & metadata](#tags--metadata)). `name` must be unique, non-empty and at most 64 characters. Caller must be admin.

**Errors:** `Unauthorized`, `RouteAlreadyExists`, `NotInitialized`, `InvalidRouteName`, `InvalidAddress` (zero address), `InvalidMetadata`, `TooManyRoutes`

```bash
stellar contract invoke --id <CORE_ID> --network testnet --source admin \
  -- register_route --caller <ADMIN> --name oracle --address <CONTRACT_ID>
```

---

### `update_route(caller, name, new_address) → Result<(), RouterError>`
Updates an existing route to point to a new address. Emits `route_updated` and `route_overwritten` events.

**Errors:** `Unauthorized`, `RouteNotFound`, `NotInitialized`

---

### `register_route_with_ttl(caller, name, address, ttl_ledgers: Option<u32>) → Result<(), RouterError>`
Registers a new route like `register_route`, but with an optional time-to-live. When `ttl_ledgers` is `Some(n)`, the route expires `n` ledgers from the current ledger sequence number; `resolve` then returns `RouteExpired` and `get_all_routes` excludes it. `None` registers a permanent route, identical to `register_route`.

**Errors:** `Unauthorized`, `RouteAlreadyExists`, `NotInitialized`, `InvalidRouteName`

```bash
stellar contract invoke --id <CORE_ID> --network testnet --source admin \
  -- register_route_with_ttl --caller <ADMIN> --name beta-integration --address <CONTRACT_ID> --ttl_ledgers 100000
```

---

### `get_route_expiry(name) → Option<u32>`
Returns the expiry ledger sequence number for `name`, or `None` if the route does not exist or has no TTL (permanent).

---

### `extend_route_ttl(caller, name, additional_ledgers: u32) → Result<(), RouterError>`
Extends a route's TTL by `additional_ledgers`. If the route already has a TTL, the new expiry stacks on top of the existing one; if the route is permanent, it gains a TTL of `additional_ledgers` ledgers from now. Must be called before the route expires.

**Errors:** `Unauthorized`, `RouteNotFound`, `RouteExpired` (route has already lapsed), `NotInitialized`

---

### `remove_route(caller, name) → Result<(), RouterError>`
Deletes a route and removes any aliases pointing to it.

**Errors:** `Unauthorized`, `RouteNotFound`, `NotInitialized`

---

### `resolve(name) → Result<Address, RouterError>`
Resolves a route name (or alias) to its contract address. Increments `total_routed`.

`resolve` always returns the address registered for `name` (after alias lookup). Route scores never change which address is returned: if `name` is unknown, paused or expired, the call fails with `RouteNotFound`, `RoutePaused` or `RouteExpired` for `name`, regardless of any scored routes. Use [`resolve_best`](#resolve_bestgroup--resultaddress-routererror) when you want score-based selection.

**Errors:** `RouterPaused`, `RouteNotFound`, `RoutePaused`, `RouteExpired` (TTL ledger exceeded)

```bash
stellar contract invoke --id <CORE_ID> --network testnet --source any \
  -- resolve --name oracle
```

---

### `resolve_best(group) → Result<Address, RouterError>`
Explicit score-based selection. Chooses the highest-composite scored route among the routes that share the tag `group` (see [Tags & metadata](#tags--metadata)) and returns its address. Paused, expired and unscored routes in the group are skipped. Increments `total_routed` on success. See [Scoring](#scoring) for the composite formula.

**Errors:** `RouterPaused`, `RouteNotFound` (no route carries the tag `group`), `RoutePaused` (every scored route in the group is paused), `RouteExpired` (every scored route in the group has expired)

```bash
stellar contract invoke --id <CORE_ID> --network testnet --source any \
  -- resolve_best --group dex
```

---

### `set_route_paused(caller, name, paused: bool) → Result<(), RouterError>`
Pauses or unpauses a specific route.

**Errors:** `Unauthorized`, `RouteNotFound`, `NotInitialized`

---

### `set_paused(caller, paused: bool) → Result<(), RouterError>`
Pauses or unpauses the entire router. Overrides individual route state.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `get_route(name) → Option<RouteEntry>`
Returns the full `RouteEntry` for `name`, or `None` if not registered.

---

### `get_all_routes() → Vec<String>`
Returns all registered, non-expired route names. Routes whose TTL has lapsed are excluded.

---

### `add_alias(caller, existing_name, alias_name) → Result<(), RouterError>`
Creates an alias for an existing route. Resolving the alias returns the original route's address.

**Errors:** `Unauthorized`, `RouteNotFound` (existing_name), `RouteAlreadyExists` (alias_name)

---

### `remove_alias(caller, alias_name) → Result<(), RouterError>`
Removes an alias.

**Errors:** `Unauthorized`, `RouteNotFound`

---

### `total_routed() → u64`
Returns the cumulative count of successful `resolve` calls.

---

### `admin() → Result<Address, RouterError>`
Returns the current admin address.

**Errors:** `NotInitialized`

---

### `transfer_admin(current, new_admin) → Result<(), RouterError>`
Transfers admin to a new address. Emits `admin_transferred`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### Tags & metadata

Route metadata is a `RouteMetadata` value (`description`, `tags`, `owner`) stored separately from the `RouteEntry`. Metadata is validated on every write: the description may be at most 256 characters, a route may carry at most 5 tags, and each tag must be non-empty, not made up only of spaces, and at most 256 bytes. Violations return `InvalidMetadata`.

---

### `get_metadata(name) → Option<RouteMetadata>`
Returns the metadata stored for `name`, or `None` if the route has no metadata or does not exist.

---

### `update_metadata(caller, name, metadata: Option<RouteMetadata>) → Result<(), RouterError>`
Replaces a route's metadata independently of its address. Passing `None` deletes the stored metadata. Caller must be admin. Emits `metadata_updated` with `(name, has_metadata)`.

**Errors:** `Unauthorized`, `NotInitialized`, `RouteNotFound`, `InvalidMetadata`

```bash
stellar contract invoke --id <CORE_ID> --network testnet --source admin \
  -- update_metadata --caller <ADMIN> --name oracle \
  --metadata '{"description":"Price oracle","tags":["oracle","price"],"owner":"<OWNER_ADDRESS>"}'
```

---

### `add_route_tag(caller, name, tag) → Result<(), RouterError>`
Adds `tag` to a route's metadata. Idempotent: adding a tag that is already present is a no-op. If the route has no metadata yet, metadata is created with an empty description and `caller` as owner. Caller must be admin. Emits `route_tag_added` when a tag is actually added.

**Errors:** `Unauthorized`, `NotInitialized`, `RouteNotFound`, `InvalidMetadata` (would exceed 5 tags, or the tag is empty, all spaces, or longer than 256 bytes)

---

### `remove_route_tag(caller, name, tag) → Result<(), RouterError>`
Removes `tag` from a route's metadata. Removing a tag that is not present (or from a route with no metadata) is a no-op as long as the route exists. Caller must be admin. Emits `route_tag_removed` when a tag is actually removed.

**Errors:** `Unauthorized`, `NotInitialized`, `RouteNotFound`

---

### `get_routes_by_tag(tag) → Vec<String>`
Returns the names of all non-expired routes whose metadata contains `tag`. Routes without metadata are skipped. O(n) over registered routes.

---

### `get_all_tags() → Vec<String>`
Returns every distinct tag used by the metadata of non-expired routes, in first-seen order. Tags that exist only on expired routes are not reported.

---

### Batches

---

### `register_routes_batch(caller, routes: Vec<RouteRegisterInput>, fail_fast: bool) → Result<BatchResult, RouterError>`
Registers several routes (each a `{ name, address }` pair) in one transaction. All routes start unpaused, permanent and without metadata. Caller must be admin.

- `fail_fast = true`: every item is validated first. At the first invalid item (duplicate name inside the batch, invalid name, zero address, name already registered, or `max_routes` cap reached) the call returns a `BatchResult` containing only that failure and **nothing is registered**.
- `fail_fast = false`: each item is processed independently. Valid items are registered and listed in `successes`; invalid items are skipped and listed in `failures` with their index and a `BatchItemError`. The batch is **not** all-or-nothing, so always inspect `failures`.

Per-item problems are reported inside the `BatchResult` (as `AlreadyExists`, `InvalidName`, or `Custom("InvalidAddress" | "TooManyRoutes" | …)`), not as a contract error.

**Errors:** `Unauthorized`, `NotInitialized`

```bash
stellar contract invoke --id <CORE_ID> --network testnet --source admin \
  -- register_routes_batch --caller <ADMIN> --fail_fast true \
  --routes '[{"name":"oracle","address":"<ORACLE_ID>"},{"name":"vault","address":"<VAULT_ID>"}]'
```

---

### `remove_routes_batch(caller, names: Vec<String>, fail_fast: bool) → Result<BatchResult, RouterError>`
Removes several routes, and any aliases pointing to them, in one transaction. Caller must be admin.

- `fail_fast = true`: every name is checked first — it must be registered, must not appear earlier in the same batch, and must have no dependents (judged against the pre-batch dependency state). At the first failing name the call returns a `BatchResult` with only that failure and **nothing is removed**.
- `fail_fast = false`: each name is processed independently; removed routes go to `successes`, and missing routes or routes still depended on (`Custom("RouteInUse")`) go to `failures`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `batch_resolve(names: Vec<String>) → Vec<BatchResolveResult>`
Resolves each name (or alias) using the same logic as `resolve` and returns one `BatchResolveResult` per input, in input order: `Ok(Address)` or `Err(ResolveError)`, where `ResolveError` is one of `RouterPaused`, `RouteNotFound`, `RoutePaused`, `NotInitialized`, `RouteExpired`. Never fails as a whole. Every successful entry increments `total_routed`. Like `resolve`, each entry returns the address registered under that name regardless of scores; use `resolve_best` for score-based selection (see [Scoring](#scoring)).

---

### Dependencies

Routes can declare other routes as prerequisites. The dependency graph is kept acyclic and is walked with a maximum recursion depth of 10. A route that other routes depend on cannot be removed (`RouteInUse`).

---

### `set_route_dependency(caller, route, depends_on) → Result<(), RouterError>`
Records `depends_on` as a direct prerequisite of `route`. Both routes must exist and the new edge must not create a cycle. Caller must be admin.

**Errors:** `Unauthorized`, `NotInitialized`, `RouteNotFound` (`route` or `depends_on`), `CircularDependency` (self-dependency or cycle), `RecursionLimitExceeded` (existing graph too deep to validate)

---

### `get_route_dependencies(route) → Result<Vec<String>, RouterError>`
Returns the direct dependencies of `route`, in the order they were added. Empty if it has none.

**Errors:** `RouteNotFound`

---

### `resolve_with_dependencies(name) → Result<Vec<(String, Address)>, RouterError>`
Resolves `name` together with its full dependency tree, returning `(route_name, address)` pairs in dependency-first order with `name` itself last. Every route in the tree must be resolvable.

**Errors:** `RouterPaused`, `RouteNotFound`, `RoutePaused`, `RouteExpired` (for `name` or any dependency), `CircularDependency`, `RecursionLimitExceeded`

---

### Scoring

Each route can carry a `RouteScore` (`liquidity_score`, `fee_bps`, `reliability_score`, each `0`–`100`). Scores are combined into a composite value:

```text
composite = liquidity_weight * liquidity_score
          + reliability_weight * reliability_score
          - fee_bps / fee_divisor
```

> **Scoring never changes what `resolve(name)` returns.** `resolve` and `batch_resolve` always return the address registered under the requested name (or its alias). Score-based selection is opt-in through the explicit APIs: `resolve_best(group)` picks the highest-composite route among the routes sharing a tag, and `get_best_route(candidates, …)` picks among an explicit candidate list. The contract also caches the highest-composite route (recomputed on every score, weight, pause and removal change) for these selection APIs; it is never substituted for a requested name.

---

### `set_route_score(caller, name, score: RouteScore) → Result<(), RouterError>`
Sets or replaces the score of an existing route and recomputes the cached best route. Caller must be admin. Emits `route_scored`.

**Errors:** `Unauthorized`, `NotInitialized`, `RouteNotFound`, `InvalidScore` (any field above 100)

```bash
stellar contract invoke --id <CORE_ID> --network testnet --source admin \
  -- set_route_score --caller <ADMIN> --name dex-a \
  --score '{"liquidity_score":90,"fee_bps":30,"reliability_score":95}'
```

---

### `set_route_scores_batch(caller, scores: Vec<RouteScoreInput>) → Result<(), RouterError>`
Sets scores for several routes (each a `{ name, score }` pair). All entries are validated before anything is written, so the call is all-or-nothing; the cached best route is recomputed once at the end. Caller must be admin.

**Errors:** `Unauthorized`, `NotInitialized`, `RouteNotFound`, `InvalidScore`

---

### `get_route_score(name) → Option<RouteScore>`
Returns the score assigned to `name`, or `None` if it has none.

---

### `get_best_route(candidates: Vec<String>, min_score: i64, fallback_name: Option<String>) → Result<Option<String>, RouterError>`
Evaluates the given candidate names and returns the one with the highest composite score. Paused, expired, missing and unscored candidates are skipped. If the best composite is below `min_score` (or no candidate qualifies), `fallback_name` is returned instead. Read-only apart from emitting `best_route_selected`.

Note: this function uses the fixed default formula `liquidity_score + reliability_score - fee_bps / 10`; it does **not** apply the weights configured with `set_scoring_weights` (those only affect the cached best route used by `resolve_best`).

**Errors:** `RouterPaused`

---

### `set_scoring_weights(caller, weights: ScoringWeights) → Result<(), RouterError>`
Sets the `liquidity_weight`, `reliability_weight` and `fee_divisor` used by the composite formula for the cached best route (used by `resolve_best`), then re-ranks all routes. Caller must be admin. Emits `scoring_weights_updated`.

**Errors:** `Unauthorized`, `NotInitialized`, `InvalidScoringWeights` (`fee_divisor <= 0`, a negative weight, or a weight above 1000)

---

### `get_scoring_weights() → ScoringWeights`
Returns the configured weights, or the defaults `{ liquidity_weight: 1, reliability_weight: 1, fee_divisor: 10 }` if none were set.

---

### Pagination, limits & stats

---

### `get_alias_target(alias_name) → Option<String>`
Returns the canonical route name that `alias_name` points to, or `None` if it is not an alias. Read-only: it does not increment `total_routed` and does not fail when the router or route is paused.

---

### `get_routes_paginated(start: u32, limit: u32) → Vec<String>`
Returns up to `limit` non-expired route names starting at zero-based index `start`, in the same order as `get_all_routes`. An out-of-range `start` or `limit = 0` returns an empty vector.

```bash
stellar contract invoke --id <CORE_ID> --network testnet --source any \
  -- get_routes_paginated --start 0 --limit 50
```

---

### `get_stats() → RouterStats`
Returns a snapshot computed in one pass over the route set: `total_routes`, `active_routes`, `paused_routes`, `expired_routes`, `alias_count`, `scored_routes`. No auth required.

---

### `route_count() → u32`
Returns the number of registered routes (aliases excluded, expired routes included). O(1).

---

### `get_max_routes() → u32`
Returns the cap on simultaneously registered routes, as set by `initialize` or `set_max_routes`. Defaults to `1000`.

---

### `set_max_routes(caller, max_routes: u32) → Result<(), RouterError>`
Changes the route cap. Lowering it below the current `route_count` removes nothing; it only blocks new registrations (`TooManyRoutes`) until the count drops below the new cap. Caller must be admin.

**Errors:** `Unauthorized`, `NotInitialized`, `InvalidRouteName` (`max_routes` is `0`)

---

## router-registry

**Contract:** `RouterRegistry`  
**Purpose:** Versioned address book — stores contract addresses keyed by `(name, version)`.

### `initialize(admin) → Result<(), RegistryError>`
**Errors:** `AlreadyInitialized`

---

### `register(caller, name, address, version: u32) → Result<(), RegistryError>`
Registers a contract entry. `version` must be > 0 and greater than all existing versions for `name`.

**Errors:** `Unauthorized`, `InvalidVersion`, `AlreadyRegistered`, `NotInitialized`

```bash
stellar contract invoke --id <REGISTRY_ID> --network testnet --source admin \
  -- register --caller <ADMIN> --name oracle --address <CONTRACT_ID> --version 1
```

---

### `register_with_check(caller, name, address, version: u32, health_fn: Option<Symbol>) → Result<(), RegistryError>`
Same as `register`, but can first probe the target contract with a zero-argument health call. Version and duplicate checks run **before** the health call, so a registration that would be rejected never invokes the target.

- `health_fn` must be `None`, `health` or `ping`. Any other symbol is rejected with `InvalidHealthFn` before any cross-contract call is made. This allow-list is a guardrail on names only: the target's `health`/`ping` must itself be side-effect-free.
- If the allow-listed function is missing on the target or the call fails, registration is rejected with `ContractUnreachable`.
- With `health_fn = None` it behaves exactly like `register`.

**Errors:** `Unauthorized`, `NotInitialized`, `InvalidVersion`, `AlreadyRegistered`, `InvalidHealthFn`, `ContractUnreachable`

---

### `bulk_register(caller, entries: Vec<BulkRegistrationInput>, fail_fast: bool) → Result<BatchResult, RegistryError>`
Registers several `{ name, address, version }` entries in one admin-authorized call. Returns a `BatchResult` with the indices that succeeded and a per-index error for each failure.

- **Limit:** at most `MAX_BULK_BATCH_SIZE` (**50**) entries per call. Larger batches are rejected with `BatchTooLarge` before anything is written.
- `fail_fast = true`: every entry is validated first (against storage *and* earlier entries in the same batch, including duplicate `(name, version)` pairs and non-ascending versions for the same name). If any entry fails, nothing is written and the result holds that single failure.
- `fail_fast = false`: entries are processed one by one; valid entries are written and invalid ones are recorded as failures.
- A `batch_executed` event is emitted when the call finishes, even if every entry failed.

**Errors (whole call):** `Unauthorized`, `NotInitialized`, `BatchTooLarge`. Per-entry failures (`InvalidVersion`, `AlreadyRegistered`, ...) are reported inside the `BatchResult`.

---

### `get(name, version: u32) → Result<ContractEntry, RegistryError>`
Returns the entry for `(name, version)`.

**Errors:** `NotFound`

---

### `is_deprecated(name, version: u32) → Result<bool, RegistryError>`
Returns whether `(name, version)` is deprecated.

**Errors:** `NotFound` (no entry for `(name, version)`)

---

### `get_latest(name) → Result<ContractEntry, RegistryError>`
Returns the highest non-deprecated version for `name`.

**Errors:** `NotFound` (no versions registered for `name`), `AllVersionsDeprecated` (every version of `name` is deprecated)

---

### `get_latest_with_constraint(name, constraint: Option<String>) → Result<ContractEntry, RegistryError>`
Returns the highest non-deprecated version matching a semver constraint (`>=X`, `<=X`, `>X`, `<X`, `^X`, `~X`, or exact).

**Errors:** `NotFound`, `AllVersionsDeprecated`, `InvalidConstraint`

---

### `deprecate(caller, name, version: u32) → Result<(), RegistryError>`
Marks a version as deprecated. Deprecated versions are skipped by `get_latest`.

**Errors:** `Unauthorized`, `VersionNotFound`, `AlreadyDeprecated`, `NotInitialized`

---

### `deprecate_all_versions(caller, name, reason: Option<String>) → Result<(), RegistryError>`
Deprecates every registered version of `name` and records `reason` on each one. Versions that are already deprecated are skipped (their existing reason is kept), so the call does not fail on them.

**Errors:** `Unauthorized`, `NotInitialized`, `NotFound` (`name` has never been registered)

---

### `deprecate_many(caller, entries: Vec<(String, u32)>) → Vec<Result<(), RegistryError>>`
Batch deprecation. Returns one result per entry; partial failures are allowed.

---

### `versions(name) → Vec<u32>`
Returns all registered version numbers for `name` in ascending order.

---

### `get_all_versions(name) → Vec<ContractEntry>`
Returns every `ContractEntry` for `name`, including deprecated ones, in ascending version order. Empty if `name` is not registered.

---

### `get_all_names() → Vec<String>`
Returns every registered contract name, in the order each name was first registered.

---

### `get_entry_by_address(address) → Option<ContractEntry>`
Reverse lookup: returns the entry registered for `address`, or `None` if that address is not registered. Constant-time, using the address index written at registration.

---

### `admin() → Result<Address, RegistryError>`
**Errors:** `NotInitialized`

---

### `transfer_admin(current, new_admin) → Result<(), RegistryError>`
**Errors:** `Unauthorized`, `NotInitialized`

---

## router-access

**Contract:** `RouterAccess`  
**Purpose:** Role-based access control with optional role expiry and blacklisting.

### `initialize(super_admin) → Result<(), AccessError>`
**Errors:** `AlreadyInitialized`

---

### `set_role_limits(caller, max_roles: Option<u32>, max_grants_per_role: Option<u32>) → Result<(), AccessError>`
Sets the caps on how many distinct role names may exist (`max_roles`, default **100**) and how many addresses may hold a single role at once (`max_grants_per_role`, default **1000**). Passing `None` for either value restores its default; `Some(n)`, including `Some(0)`, stores exactly `n`. Only the super-admin can call this. Emits `role_limits_set`.

Hitting a cap makes later calls fail with `MaxRolesExceeded` (a new role name) or `MaxGrantsPerRoleExceeded` (a new member of a full role).

**Errors:** `Unauthorized`, `NotInitialized`, `Blacklisted`

---

### `get_role_limits() → (u32, u32)`
Returns the active `(max_roles, max_grants_per_role)`. Returns the defaults `(100, 1000)` if `set_role_limits` has never been called.

---

### `grant_role(caller, role, target, expires_at: Option<u64>) → Result<(), AccessError>`
Grants `role` to `target`. `expires_at` is an optional ledger sequence number after which the role expires. Caller must be super-admin or role admin.

**Errors:** `Unauthorized`, `AlreadyHasRole`, `Blacklisted`

---

### `grant_role_batch(admin, accounts: Vec<Address>, role, expires_in: Option<u64>, fail_fast: bool) → Result<BatchResult, AccessError>`
Grants `role` to each address in `accounts`. `admin` is authorized once, up front (super-admin or the role's admin). `expires_in` is in seconds from the current ledger timestamp; `None` means the grant never expires.

- Returns `Ok(BatchResult)` whenever authorization succeeds, even if every grant fails. `successes` lists the indices that were granted; `failures` lists each failed index with its error (for example `AlreadyExists` for `AlreadyHasRole`, or `Custom("Blacklisted")`, `Custom("MaxGrantsPerRoleExceeded")`).
- `fail_fast = true` stops at the first failure; `fail_fast = false` processes every account.
- The batch is **not atomic**: grants made before a failure stay in place.

**Errors (whole call):** `Unauthorized`, `NotInitialized`, `Blacklisted` (the caller is blacklisted)

---

### `revoke_role(caller, role, target) → Result<(), AccessError>`
Revokes `role` from `target`. Removes the storage key (not just sets to false).

**Errors:** `Unauthorized`, `RoleNotFound`

---

### `has_role(role, target) → bool`
Returns `true` if `target` holds `role` and it has not expired. Returns `false` for blacklisted addresses.

See [Role hierarchy](#role-hierarchy): holding an ancestor role also satisfies `has_role` for its descendants.

---

### `is_role_expired(role, target) → bool`
Returns `true` only if `target` has a stored expiry for `role`, the current ledger timestamp is **strictly greater** than that expiry, and `target` does not hold `role` through the hierarchy. `expires_at` is the last valid timestamp. Returns `false` when there is no grant or no expiry, so it never disagrees with `has_role`.

---

### `get_role_expiry(role, target) → Option<u64>`
Returns the stored expiry timestamp for `target`'s direct grant of `role`, or `None` if there is no grant. Grants made without an expiry are stored with `u64::MAX`.

---

### `get_role_member_count(role) → u32`
Returns the number of addresses whose grant of `role` was active when last written. This is the counter checked against `max_grants_per_role`. Grants that expire on their own are not subtracted until they are revoked or re-granted. Returns `0` for unknown roles.

---

### `set_role_admin(caller, role, admin) → Result<(), AccessError>`
Designates `admin` as the address that can grant/revoke `role`. Only super-admin can call this.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `get_role_admin(role) → Option<Address>`
Returns the address set as `role`'s admin with `set_role_admin`, or `None` if there is none.

---

### `set_role_parent(caller, role, parent_role) → Result<(), AccessError>`
Makes `parent_role` the parent of `role`, so anyone holding `parent_role` (or any of its ancestors) also satisfies `has_role(role)`. Calling it again for the same `role` replaces its parent. Only the super-admin can call this. Both role names are added to `list_all_roles`. Emits `role_parent_set`.

The edge is rejected if it would create a cycle (`HierarchyCycle`) or make the ancestor chain from `role` reach `MAX_HIERARCHY_DEPTH` (16) levels (`HierarchyTooDeep`).

**Errors:** `Unauthorized`, `NotInitialized`, `Blacklisted`, `HierarchyCycle`, `HierarchyTooDeep`, `MaxRolesExceeded`

---

### `get_role_parent(role) → Option<String>`
Returns `role`'s parent role, or `None` if it has none.

---

### `list_all_roles() → Vec<String>`
Returns every role name ever recorded through `grant_role`, `grant_role_batch`, `set_role_admin`, `set_role_parent` or `transfer_role_membership`. Roles are never removed from this list, even after all members are revoked. Its length is what `max_roles` limits.

---

### `blacklist(caller, target) → Result<(), AccessError>`
Prevents `target` from being granted any role. Cannot blacklist the super-admin.

**Errors:** `Unauthorized`, `CannotBlacklistAdmin`, `NotInitialized`

---

### `unblacklist(caller, target) → Result<(), AccessError>`
Removes `target` from the blacklist.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `is_blacklisted(target) → bool`

---

### `get_role_members(role) → Vec<Address>`
Returns all addresses currently holding `role`.

---

### `get_roles_for_address(addr) → Vec<String>`
Returns all roles held by `addr`.

---

### `transfer_role_membership(caller, role, from, to) → Result<(), AccessError>`
Moves `from`'s grant of `role` to `to`, keeping the same expiry timestamp. `caller` must be the super-admin or `role`'s admin. `from` must hold the role actively (not expired). Transferring to the same address only checks that and changes nothing. Emits `role_granted` for `to` and `role_revoked` for `from`.

**Errors:** `Unauthorized`, `NotInitialized`, `Blacklisted` (caller or `to`), `RoleNotFound` (`from` has no active grant), `DestinationAlreadyHasRole`

---

### `expire_role(caller, role, target) → Result<(), AccessError>`
Removes the expiry entry for a role grant (cleanup function). Only super-admin.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `super_admin() → Result<Address, AccessError>`
**Errors:** `NotInitialized`

---

### `transfer_super_admin(current, new_admin) → Result<(), AccessError>`
**Errors:** `Unauthorized`, `NotInitialized`

---

### Role hierarchy

Roles can be arranged in a parent/child tree with `set_role_parent(caller, child, parent)`.

- `has_role(child, account)` returns `true` if `account` directly holds `child` **or any ancestor** of `child` (its parent, the parent's parent, and so on). So holding a parent role satisfies `has_role` for all of its children.
- It does not work the other way: holding a child role does not grant the parent.
- The walk is limited to `MAX_HIERARCHY_DEPTH` (**16**) levels. `set_role_parent` refuses edges that would form a cycle or a longer chain, so any accepted hierarchy is checked in full.
- Each step of the walk still checks expiry: an expired ancestor grant does not count.
- Blacklisted addresses always fail `has_role`, whatever roles they hold.
- Inherited roles are not listed by `get_role_members` or `get_roles_for_address`, which report direct grants only.

---

## router-quote

**Contract:** `RouterQuote`  
**Purpose:** Fee-based quote calculation and route comparison. Each route has a fee in basis points (a flat per-route fee, an amount-based tier schedule, or the contract-wide default), and quotes report the output left after that fee is deducted. Quotes are read-only previews; nothing is transferred.

**Fee resolution order** for a route and `amount_in`:

1. If the route has a tier schedule, the tier with the highest `min_amount` that is `<= amount_in` applies.
2. Otherwise (or if no tier matches), the route's flat fee set with `set_route_fee` applies.
3. Otherwise, the default fee applies.

All fees are in basis points: `1 bps = 0.01%`, max `10000` (100%).

### `initialize(admin, default_fee_bps: u32) → Result<(), QuoteError>`
Stores the admin and the default fee. Must be called once before anything else.

**Errors:** `AlreadyInitialized`, `InvalidFeeBps` (`default_fee_bps > 10000`)

```bash
stellar contract invoke --id <QUOTE_ID> --network testnet --source admin \
  -- initialize --admin <ADMIN_ADDRESS> --default_fee_bps 30
```

---

### `set_route_fee(caller, route, fee_bps: u32) → Result<(), QuoteError>`
Sets a flat fee for `route`. Caller must be admin. Emits `route_fee_set`.

**Errors:** `Unauthorized`, `NotInitialized`, `InvalidFeeBps`, `TooManyRoutes` (the configured-routes index already holds 500 routes)

---

### `unset_route_fee(caller, route) → Result<(), QuoteError>`
Removes both the flat fee **and** the tier schedule for `route`, so it falls back to the default fee. A no-op for routes with no custom fee. Caller must be admin. Emits `route_fee_unset`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `set_route_fee_tiers(caller, route, tiers: Vec<FeeTier>) → Result<(), QuoteError>`
Replaces the tier schedule for `route`. Tiers are stored sorted by `min_amount` ascending. Passing an empty vector clears the schedule. Caller must be admin. Emits `route_fee_tiers_set`.

**Errors:** `Unauthorized`, `NotInitialized`, `TooManyTiers` (more than 100 tiers), `InvalidFeeTier` (negative `min_amount`), `InvalidFeeBps` (a tier's `fee_bps > 10000`), `TooManyRoutes`

```bash
stellar contract invoke --id <QUOTE_ID> --network testnet --source admin \
  -- set_route_fee_tiers --caller <ADMIN> --route xlm-usdc \
  --tiers '[{"min_amount":"0","fee_bps":30},{"min_amount":"1000000000","fee_bps":20}]'
```

---

### `get_route_fee(route) → Result<u32, QuoteError>`
Returns the route's flat fee, or the default fee if none is set. Does **not** consider tiers; use `get_quote` for the fee that actually applies to a given amount.

**Errors:** `NotInitialized`

---

### `get_route_fee_tiers(route) → Result<Vec<FeeTier>, QuoteError>`
Returns the route's tier schedule sorted by `min_amount` ascending, or an empty vector if it has none.

**Errors:** `NotInitialized`

---

### `get_all_configured_routes() → Vec<(String, u32)>`
Returns `(route, fee_bps)` for every route that has ever had a flat fee or tier schedule set. For tiered routes the reported fee is the lowest tier's fee (the tier with the smallest `min_amount`); otherwise it is the flat fee, falling back to the default fee. Routes stay in the index after `unset_route_fee` and are then reported with the default fee.

---

### `get_quote(request: QuoteRequest) → Result<QuoteResponse, QuoteError>`
Quotes one route. The fee is resolved as described above, then:

```text
fee_amount = floor(amount_in * fee_bps / 10_000)
amount_out = amount_in - fee_amount
```

Emits `quote_calculated`.

**Errors:** `NotInitialized`, `InvalidAmount` (`amount_in <= 0`), `ArithmeticOverflow`

```bash
stellar contract invoke --id <QUOTE_ID> --network testnet --source any \
  -- get_quote \
  --request '{"route":"xlm-usdc","token_in":"<TOKEN_IN>","token_out":"<TOKEN_OUT>","amount_in":"10000000"}'
```

---

### `get_quotes(requests: Vec<QuoteRequest>) → Result<Vec<QuoteResponse>, QuoteError>`
Quotes every request, in order. Fails as a whole if any single request fails.

**Errors:** `NoQuotesProvided` (empty vector), `NotInitialized`, `InvalidAmount`, `ArithmeticOverflow`

---

### `get_best_quote(requests: Vec<QuoteRequest>) → Result<QuoteResponse, QuoteError>`
Quotes every request and returns the one with the highest `amount_out` (the first one wins ties). Fails as a whole if any request fails. Emits `best_quote_selected`.

**Errors:** `NoQuotesProvided`, `NotInitialized`, `InvalidAmount`, `ArithmeticOverflow`

---

### `get_quotes_sorted(requests: Vec<QuoteRequest>) → Result<Vec<QuoteResponse>, QuoteError>`
Quotes every request, **silently drops** the ones that fail (e.g. `amount_in <= 0`, overflow), and returns the rest sorted by `amount_out` descending. Returns an empty vector if every request fails.

**Errors:** `NoQuotesProvided`, `NotInitialized`

---

### `set_default_fee(caller, fee_bps: u32) → Result<(), QuoteError>`
Changes the default fee used by routes without a flat fee or matching tier. Caller must be admin. Emits `default_fee_updated`.

**Errors:** `Unauthorized`, `NotInitialized`, `InvalidFeeBps`

---

### `get_default_fee() → Result<u32, QuoteError>`
Returns the default fee in basis points.

**Errors:** `NotInitialized`

---

### `admin() → Result<Address, QuoteError>`
**Errors:** `NotInitialized`

---

### `transfer_admin(current, new_admin) → Result<(), QuoteError>`
Transfers admin to `new_admin`. Emits `admin_transferred`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### Types

#### `QuoteRequest`
| Field | Type | Description |
|---|---|---|
| `route` | `String` | Route name whose fee configuration is used |
| `token_in` | `Address` | Input token (echoed back; does not affect the fee) |
| `token_out` | `Address` | Output token (echoed back; does not affect the fee) |
| `amount_in` | `i128` | Input amount in base units; must be `> 0` |

#### `QuoteResponse`
| Field | Type | Description |
|---|---|---|
| `route` | `String` | Route name |
| `token_in` | `Address` | Input token |
| `token_out` | `Address` | Output token |
| `amount_in` | `i128` | Input amount |
| `amount_out` | `i128` | `amount_in - fee_amount` |
| `fee_amount` | `i128` | Fee deducted, in input-token units |
| `fee_bps` | `u32` | Fee rate that was applied |

#### `FeeTier`
| Field | Type | Description |
|---|---|---|
| `min_amount` | `i128` | Smallest `amount_in` this tier applies to; must be `>= 0` |
| `fee_bps` | `u32` | Fee for this tier; `<= 10000` |

---

### `QuoteError`

| Error | Value | When it occurs | How to handle it |
| --- | ---: | --- | --- |
| `AlreadyInitialized` | 1 | `initialize` is called a second time. | Treat as already configured. |
| `NotInitialized` | 2 | Any fee lookup or quote runs before `initialize`. | Initialize the contract first. |
| `Unauthorized` | 3 | An admin-only function is called by a non-admin. | Sign with the admin account. |
| `InvalidAmount` | 4 | `amount_in <= 0`. | Send a positive amount. |
| `InvalidFeeBps` | 5 | A fee (default, route or tier) is above `10000`. | Use a value in `0..=10000`. |
| `NoQuotesProvided` | 6 | `get_quotes`, `get_best_quote` or `get_quotes_sorted` receives an empty vector. | Pass at least one request. |
| _(retired)_ | 7 | Formerly `RouteNotFound`. Never returned; the value is reserved so it is not confused with errors from old deployments. | — |
| `ArithmeticOverflow` | 8 | `amount_in * fee_bps` overflows `i128`. | Use a smaller amount. |
| `TooManyRoutes` | 9 | Setting a fee or tiers for a new route when 500 routes are already tracked. | Reuse existing route names. |
| `InvalidFeeTier` | 10 | A tier has a negative `min_amount`. | Use `min_amount >= 0`. |
| `TooManyTiers` | 11 | More than 100 tiers are passed to `set_route_fee_tiers`. | Use at most 100 tiers. |

---

### Rounding

All quote arithmetic uses integer division, which truncates toward zero. The result is deterministic and never rounds up:

```text
fee_amount = floor(amount_in * fee_bps / 10_000)
amount_out = amount_in - fee_amount
```

Because the fee is rounded down, `amount_out` is rounded in the user's favour, and very small amounts can have a zero fee (for example `amount_in = 33`, `fee_bps = 30` gives `fee_amount = 0`). Amounts are in token base units; the contract knows nothing about token decimals. Keep the integer values for transaction decisions and apply decimal formatting only when displaying them. To compare quotes, compare `amount_out` integers directly instead of converting to floating point.

| `amount_in` | `fee_bps` | `fee_amount` | `amount_out` |
|---:|---:|---:|---:|
| `1_000_000` | `30` | `3_000` | `997_000` |
| `333` | `30` | `0` | `333` |
| `3_334` | `30` | `10` | `3_324` |

---

## router-middleware

**Contract:** `RouterMiddleware`  
**Purpose:** Pre/post call hooks with per-caller rate limiting, a per-route circuit breaker, and a per-route call log.

View functions use the `get_` prefix (issue #1328). The old names `admin()`, `total_calls()`, `route_config()` and `rate_limit_state()` are not part of the contract interface; use `get_admin`, `get_total_calls`, `get_route_config` and `get_rate_limit_state`.

### Setup & admin

---

### `initialize(admin) → Result<(), MiddlewareError>`
Stores the admin, enables middleware globally and sets the call counter to zero.

**Errors:** `AlreadyInitialized`

---

### `get_admin() → Result<Address, MiddlewareError>`
Returns the current admin.

**Errors:** `NotInitialized`

---

### `transfer_admin(current, new_admin) → Result<(), MiddlewareError>`
Transfers admin to `new_admin`. Emits `admin_transferred`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `set_global_enabled(caller, enabled: bool) → Result<(), MiddlewareError>`
Globally enables or disables middleware. While disabled, every `pre_call` returns `MiddlewareDisabled`. Caller must be admin. Emits `middleware_enabled`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `get_global_enabled() → bool`
Returns whether middleware is globally enabled (`true` if never set).

---

### Route configuration

---

### `configure_route(caller, route, max_calls_per_window: u32, window_seconds: u64, enabled: bool, failure_threshold: u32, recovery_window_seconds: u64, log_retention: u32, burst_allowance: u32) → Result<(), MiddlewareError>`
Creates or replaces the middleware config for `route` and adds it to the configured-routes list. Caller must be admin. Emits `route_configured`.

- `max_calls_per_window = 0` disables route-level rate limiting.
- `burst_allowance` extra calls are allowed per window on top of `max_calls_per_window`.
- `failure_threshold = 0` disables the circuit breaker.
- `log_retention` is the call-log ring-buffer capacity (`0` disables logging, max `10000`). Changing it clears the existing log and its summary; leaving it unchanged keeps them.

**Errors:** `Unauthorized`, `NotInitialized`, `InvalidConfig` (`window_seconds = 0` with `max_calls_per_window > 0`, or `log_retention > 10000`)

```bash
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source admin \
  -- configure_route \
  --caller <ADMIN> --route oracle/get_price \
  --max_calls_per_window 100 --window_seconds 3600 \
  --enabled true --failure_threshold 5 --recovery_window_seconds 300 \
  --log_retention 100 --burst_allowance 10
```

---

### `get_route_config(route) → Option<RouteConfig>`
Returns the config for `route`, or `None` if it was never configured.

---

### `get_configured_routes() → Vec<String>`
Returns every route name that has been passed to `configure_route`.

---

### Hooks

---

### `pre_call(caller, route) → Result<(), MiddlewareError>`
Must be called (and authorized by `caller`) before routing. Checks, in order: global enable, route enable, the circuit breaker, then the rate limit. Routes that were never configured only get the global-enable check. On success increments the total-call counter and emits `pre_call`.

`pre_call` no longer takes a per-route reentrancy guard: it makes no cross-contract calls, and Soroban rejects contract re-entry at the host level, so the guard could never fire. `Reentrancy` is retired and never returned.

**Errors:** `MiddlewareDisabled`, `RouteDisabled`, `CircuitOpen`, `RateLimitExceeded` (only under the `Reject` strategy)

---

### `post_call(caller, route, success: bool)`
Must be called (and authorized by `caller`) after routing. Emits `post_call`, appends to the route's call log (if enabled), and feeds the circuit breaker: a failure increments the failure count and may open the circuit, a success closes a half-open circuit (or clears the failure count of a closed one). Does nothing beyond the event for unconfigured routes.

---

### `reset_guard(caller, route) → Result<(), MiddlewareError>`
**Deprecated — no-op.** Formerly an emergency escape hatch that cleared a stuck reentrancy guard for `route`. The guard has been removed, so this function does nothing beyond the admin authorization check; it is kept only for ABI compatibility. Caller must be admin. Emits `guard_reset`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `get_total_calls() → u64`
Cumulative count of successful `pre_call` invocations.

---

### Rate limiting

See [Rate Limiting Algorithm](#rate-limiting-algorithm) below for how windows are counted.

---

### `get_rate_limit_state(route, caller) → Option<RateLimitState>`
Returns `caller`'s rate-limit state on `route`, or `None` if the caller has no state yet. If the caller's effective window (per-caller override window when set, otherwise the route's `window_seconds`) has elapsed, the returned view shows `calls_in_window = 0` and `window_start = now` — what the next `pre_call` would see. The recomputed view is not persisted.

---

### `get_rate_limit_state_for_caller(route, caller) → Option<RateLimitState>`
Alias of `get_rate_limit_state` (issue #1328).

---

### `get_rate_limit_stats(route, caller) → Option<RateLimitState>`
Alias of `get_rate_limit_state`; returns the same value.

---

### `get_route_rate_limit_stats(route) → Option<RouteRateLimitStats>`
Aggregates rate-limit state across all callers of `route`: the sum of `calls_in_window` (using each caller's effective window), the earliest `window_start`, and the sum of `total_violations`. Returns `None` if no caller has state on the route.

---

### `reset_rate_limit(caller, route, target_caller) → Result<(), MiddlewareError>`
Clears `target_caller`'s rate-limit state on `route` (window counter and `total_violations`). Caller must be admin. Emits `rate_limit_reset`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `set_rate_limit_strategy(caller, route, strategy: RateLimitStrategy) → Result<(), MiddlewareError>`
Sets what happens when a caller goes over the limit on `route`:

| Strategy | `pre_call` result | Side effects |
|---|---|---|
| `Reject` (default) | `Err(RateLimitExceeded)` | none (the failed call's writes roll back) |
| `Throttle` | `Ok` | emits `rate_limit_throttled`, increments `total_violations` |
| `LogOnly` | `Ok` | emits `rate_limit_exceeded`, increments `total_violations` |

Caller must be admin. Emits `rate_limit_strategy_set`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `get_rate_limit_strategy(route) → RateLimitStrategy`
Returns the strategy for `route` (`Reject` if never set).

---

### `configure_caller_rate_limit(caller, route, target_caller, max_calls: u32, window_secs: u64) → Result<(), MiddlewareError>`
Sets a per-caller override for `target_caller` on `route`. While present, `pre_call` uses `max_calls` / `window_secs` for that caller instead of the route's `max_calls_per_window + burst_allowance` / `window_seconds`. Overrides apply even on routes whose route-level limit is `0` (unlimited). An override of `max_calls = 0` makes that caller **unlimited**, exempting them from the route-level limit too. Caller must be admin. Emits `caller_rate_limit_set`.

**Errors:** `Unauthorized`, `NotInitialized`, `InvalidConfig` (`window_secs = 0` with `max_calls > 0`)

```bash
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source admin \
  -- configure_caller_rate_limit \
  --caller <ADMIN> --route oracle/get_price --target_caller <PARTNER_ADDRESS> \
  --max_calls 1000 --window_secs 3600
```

---

### `get_caller_rate_limit(route, target_caller) → Option<CallerRateLimitConfig>`
Returns the override for `target_caller` on `route`, or `None` if the route-level limit applies.

---

### `remove_caller_rate_limit(caller, route, target_caller) → Result<(), MiddlewareError>`
Deletes the override so `target_caller` falls back to the route-level limit. Caller must be admin. Emits `caller_rate_limit_removed`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### `check_caller_rate_limit(route, caller) → Result<bool, MiddlewareError>`
Read-only check against `caller`'s per-caller override: `Ok(true)` if the next call is within the override's limit, `Ok(false)` if it is exhausted. Returns `Ok(true)` when there is no override or the override is unlimited (`max_calls = 0`). It does not evaluate the route-level limit.

---

### Circuit breaker

---

### `get_circuit_breaker_state(route) → Option<CircuitBreakerState>`
Returns the circuit-breaker state for `route`, or `None` if it has none yet. If the circuit is open and its recovery window has elapsed, the view reports `is_open = false`, `is_half_open = true` (the transition the next `pre_call` will make) without persisting it.

---

### `reset_circuit_breaker(caller, route) → Result<(), MiddlewareError>`
Closes the circuit for `route` and clears its failure count. Caller must be admin. Emits `circuit_reset`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### Call log

Each configured route with `log_retention > 0` keeps a ring buffer of its most recent `post_call` results plus an incrementally maintained summary.

---

### `get_call_log(route) → Vec<CallLogEntry>`
Returns the retained entries for `route`, oldest first. Empty if logging is disabled or nothing was logged.

---

### `get_call_log_filtered(route, success_only: bool) → Vec<CallLogEntry>`
Like `get_call_log`, but returns only successful entries when `success_only = true`, or only failed entries when `false`.

---

### `get_call_log_length(route) → u32`
Returns the number of entries currently retained (at most `log_retention`).

---

### `get_call_log_summary(route) → Option<CallLogSummary>`
Returns `total_calls`, `success_count`, `failure_count` and `last_call_timestamp` for `route`, or `None` if nothing was logged since the last reset.

---

### `reset_route_call_log(caller, route) → Result<(), MiddlewareError>`
Clears both the call log and its summary for `route`. Caller must be admin. Emits `call_log_cleared`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### Types

#### `RouteConfig`
| Field | Type | Description |
|---|---|---|
| `max_calls_per_window` | `u32` | Route-level limit per caller per window (`0` = unlimited) |
| `window_seconds` | `u64` | Window length |
| `enabled` | `bool` | `false` makes `pre_call` return `RouteDisabled` |
| `failure_threshold` | `u32` | Failures before the circuit opens (`0` = breaker disabled) |
| `recovery_window_seconds` | `u64` | Time an open circuit waits before going half-open |
| `log_retention` | `u32` | Call-log capacity (`0` = disabled, max `10000`) |
| `burst_allowance` | `u32` | Extra calls allowed above `max_calls_per_window` |

#### `RateLimitState`
| Field | Type | Description |
|---|---|---|
| `calls_in_window` | `u32` | Calls counted in the current window |
| `window_start` | `u64` | Ledger timestamp the window started |
| `total_violations` | `u32` | Over-limit calls let through under `Throttle` / `LogOnly` |

#### `RouteRateLimitStats`
| Field | Type | Description |
|---|---|---|
| `total_calls_in_window` | `u32` | Sum of current-window calls across callers |
| `window_start` | `u64` | Earliest window start across callers |
| `total_violations` | `u32` | Sum of violations across callers |

#### `CallerRateLimitConfig`
| Field | Type | Description |
|---|---|---|
| `max_calls` | `u32` | Per-caller limit (`0` = unlimited) |
| `window_secs` | `u64` | Per-caller window length |

#### `CircuitBreakerState`
| Field | Type | Description |
|---|---|---|
| `failure_count` | `u32` | Consecutive failures |
| `opened_at` | `u64` | Timestamp the circuit opened |
| `is_open` | `bool` | Calls are blocked |
| `is_half_open` | `bool` | Probe mode after the recovery window |

#### `CallLogEntry`
| Field | Type | Description |
|---|---|---|
| `caller` | `Address` | Caller passed to `post_call` |
| `timestamp` | `u64` | Ledger timestamp |
| `success` | `bool` | Outcome passed to `post_call` |
| `route` | `String` | Route name |

#### `CallLogSummary`
| Field | Type | Description |
|---|---|---|
| `total_calls` | `u32` | Calls logged since last reset |
| `success_count` | `u32` | Successful calls |
| `failure_count` | `u32` | Failed calls |
| `last_call_timestamp` | `u64` | Timestamp of the latest logged call |

#### `RateLimitStrategy`
`Reject` · `Throttle` · `LogOnly` — see `set_rate_limit_strategy`.

---

### Rate Limiting Algorithm

#### How It Works

The rate limiter uses a **fixed-window counter** per `(route, caller)` pair.
Each caller has its own independent window — windows are not shared or aligned
across callers.

**Window lifecycle:**

1. On the first call, `window_start` is set to the current ledger timestamp
   and `calls_in_window` is set to 1.
2. Subsequent calls within the same window increment `calls_in_window`.
3. When `calls_in_window` reaches the effective limit —
   `max_calls_per_window + burst_allowance`, or the per-caller override's
   `max_calls` when one applies — the call is over the limit. Under the
   default `Reject` strategy, `pre_call` returns `RateLimitExceeded`; because
   Soroban rolls back all storage writes of a failed invocation, the rejected
   call is **not** recorded in `total_violations`. Under `Throttle` and
   `LogOnly`, `pre_call` returns `Ok`, so the over-limit call is allowed
   through and `total_violations` is incremented. `total_violations` therefore
   counts over-limit calls that were allowed through, not rejected ones.
4. On the first call after `window_start + window_seconds` has elapsed, the
   window resets: `window_start = now`, `calls_in_window = 1`.

The check in `pre_call`:
```
window_elapsed = now >= window_start + window_seconds
calls           = 0               if window_elapsed else calls_in_window
window_start    = now             if window_elapsed else window_start
```

#### Ledger Timestamp Granularity

Stellar closes a ledger approximately every **5 seconds**. All calls within
the same ledger closure share the same `env.ledger().timestamp()` value. This
means:

- A `window_seconds = 60` window allows all calls within the same 5-second
  ledger to be counted as a single moment. Setting `max_calls_per_window = 10`
  does **not** prevent 10 calls in the same ledger.
- For burst protection, set `window_seconds` smaller than 1 ledger close time
  only if you want to block multiple calls per ledger (they will all see the
  same `now` and the first call sets the window, subsequent ones within the
  same ledger see `window_elapsed = false`).

#### Tuning Guidelines

| Traffic pattern | Suggested config | Notes |
|---|---|---|
| Low-traffic route | `max_calls=10, window_secs=60` | 10 calls per minute per caller |
| High-traffic route | `max_calls=1000, window_secs=300` | 1000 calls per 5 minutes per caller |
| Burst protection | `max_calls=5, window_secs=5` | 5 calls per 5-second ledger window |
| Effectively unlimited | `max_calls_per_window=0` | Rate limiting disabled for route |

**Short window + low count** vs **long window + high count:**

- Short windows (e.g., 5s / 5 calls) aggressively block bursts but reset
  quickly, allowing another burst immediately after.
- Long windows (e.g., 3600s / 100 calls) smooth traffic over time but a
  burst at window-start consumes all capacity until the window expires.

Choose based on whether you need burst suppression (short) or sustained
throughput limiting (long).

#### Gotchas

- **Windows are not aligned across routes or callers.** Each `(route, caller)`
  window starts independently on first call, so two callers can be in different
  phases of their windows simultaneously.
- **A network halt extends the effective window.** If the Stellar network
  halts for 10 minutes, `now` jumps forward by the halt duration when it
  resumes. Any caller whose window had not yet expired will suddenly find
  their window expired, resetting the counter immediately on the next call.
- **Changing `max_calls` takes effect immediately without resetting counters.**
  If you lower `max_calls_per_window` from 100 to 10 and a caller has already
  made 15 calls in the current window, they will be blocked until the window
  expires — even though those calls were made under the old config.
- **`window_seconds = 0` with `max_calls_per_window > 0` is rejected** by
  `configure_route` with `InvalidConfig`. Set `max_calls_per_window = 0` to
  disable rate limiting entirely.

---

## router-timelock

**Contract:** `RouterTimelock`  
**Purpose:** Delayed execution queue — all sensitive changes must wait a configurable delay.

Operations are identified by `op_id: Bytes`, the 32-byte SHA-256 of
`description` + `target` + `eta` (XDR-encoded description and target, then
`eta` as big-endian `u64`). The CLI prints and accepts it as a hex string.
An operation becomes executable at `eta = ledger_timestamp + delay` and
expires after `eta + grace_period_seconds`.

### `initialize(admin, min_delay: u64, max_pending_ops: u32) → Result<(), TimelockError>`
`min_delay` (seconds) must be > 0. `max_pending_ops` caps the number of
pending operations; `0` means unlimited.

**Errors:** `AlreadyInitialized`, `DelayTooShort`

---

### `queue(proposer, description: String, target, delay: u64, grace_period_seconds: u64, deps: Vec<Bytes>) → Result<Bytes, TimelockError>`
Queues a new operation and returns its `op_id`. Admin only (`proposer` must be
the admin and sign). `delay` must be >= `min_delay`; `grace_period_seconds`
must be <= 30 days (2 592 000). `deps` lists operation IDs that must be
executed before this one; each must exist, must not be this operation, and
the dependency chain may be at most 9 operations long.

Emits `op_queued` with `(op_id, target, eta, grace_period_seconds)`.

**Errors:** `NotInitialized`, `Unauthorized`, `DelayTooShort`,
`GracePeriodTooLong`, `QueueFull`, `DelayTooLong`, `AlreadyQueued`,
`CircularDependency`, `NotFound` (unknown dependency), `DependencyTooDeep`

```bash
stellar contract invoke --id <TIMELOCK_ID> --network testnet --source admin \
  -- queue \
  --proposer <ADMIN> --description "upgrade oracle to v2" \
  --target <CONTRACT_ID> --delay 86400 --grace_period_seconds 604800 \
  --deps '[]'
```

---

### `execute(caller, op_id: Bytes) → Result<(), TimelockError>`
Executes a queued operation once `eta` has passed and before
`eta + grace_period_seconds`. Admin only. All dependencies must already be
executed. Emits `op_executed` with `(op_id, target)`.

**Errors:** `NotInitialized`, `Unauthorized`, `NotFound`, `Cancelled`,
`AlreadyExecuted`, `NotReady`, `ExpiryOverflow`, `Expired`,
`DependencyNotExecuted`

---

### `cancel(caller, op_id: Bytes) → Result<(), TimelockError>`
Cancels a pending operation. Admin only. Emits `op_cancelled` with `op_id`.

**Errors:** `NotInitialized`, `Unauthorized`, `NotFound`, `Cancelled`,
`AlreadyExecuted`

---

### `update_description(caller, op_id: Bytes, new_description: String) → Result<(), TimelockError>`
Replaces the description of a pending operation. Admin only. The `op_id` is
not recomputed. Emits `op_description_updated` with `(op_id, new_description)`.

**Errors:** `NotInitialized`, `Unauthorized`, `NotFound`, `AlreadyExecuted`,
`Cancelled`

---

### `cleanup_expired(caller, limit: u32) → Result<u32, TimelockError>`
Permissionless (`caller` must sign but need not be the admin). Removes up to
`limit` operations that can never execute again — expired ones from the
pending index, and the stored data of expired, executed and cancelled ones.
An operation that is still a dependency of a live pending operation is
skipped. Returns the number removed and emits `ops_cleaned` with that count.

---

### `get_op(op_id: Bytes) → Option<Op>`
Returns the stored operation, or `None` if not found (or already cleaned up).

---

### `get_dependencies(op_id: Bytes) → Vec<Bytes>`
Returns the dependency IDs recorded for `op_id`, or an empty list.

---

### `get_operation_status(op_id: Bytes) → Option<OperationStatus>`
Returns `Cancelled`, `Executed`, `Expired`, `Blocked` (a dependency can
never execute), `Ready` or `Queued`, or `None` if the operation doesn't exist.

---

### `get_pending_operations() → Vec<Op>`
Returns operations that are not executed, not cancelled, and not expired.

---

### `get_operation_count_by_status(status: OperationStatus) → u32`
Counts pending-index operations with the given status. Always `0` for
`Executed` and `Cancelled`, since those leave the pending index.

---

### `get_operations_by_status(status: OperationStatus) → Vec<(Bytes, Op)>`
Returns `(op_id, Op)` pairs from the pending index with the given status.
Always empty for `Executed` and `Cancelled`.

---

### `get_max_pending_ops() → u32`
Returns the pending-operations cap (`0` = unlimited).

---

### `min_delay() → Result<u64, TimelockError>`
**Errors:** `NotInitialized`

---

### `set_min_delay(caller, new_min_delay: u64) → Result<(), TimelockError>`
Updates the minimum delay. Admin only. Does not affect already-queued
operations. Emits `min_delay_updated` with `(old_min_delay, new_min_delay)`.

**Errors:** `NotInitialized`, `Unauthorized`, `DelayTooShort`

---

### `admin() → Result<Address, TimelockError>`
**Errors:** `NotInitialized`

---

### `transfer_admin(current, new_admin) → Result<(), TimelockError>`
**Errors:** `NotInitialized`, `Unauthorized`

---

### `Op`

| Field | Type | Description |
|---|---|---|
| `proposer` | `Address` | Address that queued the operation |
| `description` | `String` | Human-readable description |
| `target` | `Address` | Address the operation applies to |
| `eta` | `u64` | Ledger timestamp after which it can execute |
| `grace_period_seconds` | `u64` | Execution window after `eta` |
| `executed` | `bool` | Whether `execute` succeeded |
| `cancelled` | `bool` | Whether `cancel` was called |

---

## router-multicall

**Contract:** `RouterMulticall`  
**Purpose:** Batch multiple cross-contract calls in a single transaction.

### `initialize(admin, max_batch_size: u32) → Result<(), MulticallError>`
`max_batch_size` must be > 0.

**Errors:** `AlreadyInitialized`, `InvalidConfig`

---

### `execute_batch(caller, calls: Vec<CallDescriptor>, simulate: bool) → Result<BatchSummary, MulticallError>`
Executes a batch of calls. Any authenticated address can call this (not admin-only). If `simulate = true`, calls are attempted but `total_batches` is not incremented. If a call with `required = true` fails, the batch aborts immediately.

**Failure reasons.** Each failed call is reported with a `reason`:

- `budget_exceeded` — only when the host error is a budget error (`ScErrorType::Budget`). `instruction_budget` is not applied to the invocation, so this is rare and reflects the transaction's overall budget.
- `invoke_failed` — every other failure (target returned a contract error, panicked, or does not exist), **regardless of whether `instruction_budget` was set**.

**`max_total_gas`.** When `max_total_gas` is `Some(limit)`, every call in the batch must declare an `instruction_budget`; a call with `instruction_budget: None` is rejected with `InvalidConfig` before anything is invoked. The declared budgets are summed (saturating) and the batch is rejected with `GasLimitExceeded` if the total exceeds `limit`. Note that declared budgets are only checked, not enforced: the calls still run under the transaction's overall budget.

**Errors:** `EmptyBatch`, `BatchTooLarge`, `RequiredCallFailed`, `NotInitialized`, `InvalidConfig` (`max_total_gas` is `Some` and a call has no `instruction_budget`), `GasLimitExceeded`

---

### `set_max_batch_size(caller, max_batch_size: u32) → Result<(), MulticallError>`
**Errors:** `Unauthorized`, `InvalidConfig`, `NotInitialized`

---

### `total_batches() → u64`
Cumulative count of non-simulated `execute_batch` calls.

---

### `max_batch_size() → Result<u32, MulticallError>`
**Errors:** `NotInitialized`

---

### `admin() → Result<Address, MulticallError>`
**Errors:** `NotInitialized`

---

### `get_batch_result(batch_id: u64, call_index: u32) → Result<Option<CallResult>, MulticallError>`
Returns the stored result of one call in a batch, or `None` if nothing is stored at that position. Results are only stored when `execute_batch` ran with `store_results = true`. Never returns an error today.

**Retention:** only the most recent `MAX_STORED_BATCHES` (**50**) non-simulated batches keep their results. Each new batch removes the results of the batch 51 positions back, so older lookups return `None`.

---

### `get_batch_results(batch_id: u64) → Result<Vec<CallResult>, MulticallError>`
Returns all stored results for `batch_id` in call order, starting at index 0 and stopping at the first missing index. Returns an empty list if the batch did not store results or has been pruned. The same 50-batch retention applies. Never returns an error today.

---

### `transfer_admin(current, new_admin) → Result<(), MulticallError>`
**Errors:** `Unauthorized`, `NotInitialized`

---

## router-execution

**Contract:** `RouterExecution`  
**Purpose:** Execution pipeline that invokes a target contract function with optional pre-flight "simulation", retry with exponential backoff hints, fee estimation, structured error events, and a bounded execution history. Its counters back the metrics exporter's `router_execution_*` metrics.

> **`simulate()` and `simulate_first` really invoke the target.** Soroban has no contract-level preview, so the "simulation" is a genuine `try_invoke_contract` call. When it succeeds, every state change made by the target **persists**. Only simulate read-only target functions. In `execute`, a successful simulation counts as the first attempt and is not invoked a second time.

### `initialize(admin, max_retries: u32, backoff_base_ms: u64, backoff_multiplier: u32) → Result<(), ExecutionError>`
Stores the admin, retry cap and backoff configuration, and zeroes the counters.

- `max_retries`: global cap on retries per request, `1`–`5`.
- `backoff_base_ms`: delay hint before the first retry (`0` = no delay).
- `backoff_multiplier`: growth per retry in fixed point ×100 (`100` = 1.0×, `200` = 2.0×), `100`–`10000`.

**Errors:** `AlreadyInitialized`, `InvalidConfig`

```bash
stellar contract invoke --id <EXECUTION_ID> --network testnet --source admin \
  -- initialize --admin <ADMIN_ADDRESS> --max_retries 3 \
  --backoff_base_ms 500 --backoff_multiplier 200
```

---

### `execute(caller, request: ExecutionRequest) → Result<ExecutionResult, ExecutionError>`
Invokes `request.function` on `request.target` with `request.args`. `caller` must authorize.

1. If `request.simulate_first` is `true`, the target is invoked once as a simulation. If that fails, the call stops with `SimulationFailed`. If it succeeds, that invocation **is** attempt 1 and the call returns success without invoking again.
2. Otherwise the target is invoked; any failure is retried up to `min(request.max_retries, max_retries())` more times. Before each retry an `execution_retry` event carries `delay_ms = backoff_base_ms * (backoff_multiplier / 100)^(attempt - 1)`, capped at 1 hour. The contract does not wait itself; off-chain orchestrators use this value.
3. On success: increments `total_executions`, appends an `ExecutionRecord` whose `fee_paid` is `base_fee + resource_fee` (see `estimate_fee`, without surge), and emits `execution_result`.
4. If every attempt fails: emits `execution_error` and returns `ContractRejected`.

Because a failed contract call rolls back its storage writes, the error counter and history entries written on the failure paths do not persist on-chain; only the emitted events are observable.

**Errors:** `NotInitialized`, `InvalidAmount` (`request.amount <= 0`), `ArgsTooLarge` (more than 20 args), `SimulationFailed`, `ContractRejected`

```bash
stellar contract invoke --id <EXECUTION_ID> --network testnet --source caller \
  -- execute --caller <CALLER_ADDRESS> \
  --request '{"target":"<TARGET_ID>","function":"ping","simulate_first":false,"max_retries":2,"args":[],"amount":"1000000"}'
```

---

### `estimate_fee(target, function, amount: i128, caller_asserted_load_bps: u32) → Result<FeeEstimate, ExecutionError>`
Returns a fee estimate in stroops. Depends only on `amount` and `caller_asserted_load_bps`; `target` and `function` are accepted for symmetry with `execute` and are ignored.

```text
base_fee         = 100
resource_fee     = max(amount / 1000, 100)
surge_multiplier = 200 if caller_asserted_load_bps >= 8000 else 100
total_fee        = (base_fee + resource_fee) * surge_multiplier / 100
```

`caller_asserted_load_bps` is whatever the caller passes in; it is **not** a measured network load, so `high_load` and the `fee_estimated` event only reflect the caller's own assertion. No auth required.

**Errors:** `NotInitialized`, `InvalidAmount` (`amount <= 0`)

```bash
stellar contract invoke --id <EXECUTION_ID> --network testnet --source any \
  -- estimate_fee --target <TARGET_ID> --function swap \
  --amount 10000000 --caller_asserted_load_bps 5000
```

---

### `simulate(caller, target, function, args: Vec<Val>) → Result<SimulationResult, ExecutionError>`
Invokes `function` on `target` **for real** and reports whether it succeeded (`success`, `would_fail = !success`, and a human-readable `message`). State changes made by a successful call persist — see the warning above. The target's failure is reported in the result, not as an error. `caller` must authorize. Emits `simulation_result`.

**Errors:** `NotInitialized`, `ArgsTooLarge` (more than 20 args)

---

### `set_backoff_config(caller, backoff_base_ms: u64, backoff_multiplier: u32) → Result<(), ExecutionError>`
Updates the retry backoff hint. Caller must be admin. Emits `backoff_config_updated` with old and new values.

**Errors:** `Unauthorized`, `NotInitialized`, `InvalidConfig` (`backoff_multiplier` outside `100`–`10000`)

---

### `backoff_config() → Result<(u64, u32), ExecutionError>`
Returns `(backoff_base_ms, backoff_multiplier)`.

**Errors:** `NotInitialized`

---

### `set_max_retries(caller, new_max: u32) → Result<(), ExecutionError>`
Updates the global retry cap. Caller must be admin. Emits `max_retries_updated`.

**Errors:** `Unauthorized`, `NotInitialized`, `InvalidConfig` (`new_max` is `0` or above `5`)

---

### `max_retries() → Result<u32, ExecutionError>`
Returns the global retry cap.

**Errors:** `NotInitialized`

---

### `set_max_history_size(caller, new_max: u32) → Result<(), ExecutionError>`
Sets how many `ExecutionRecord`s are kept (default `1000`). If the history is already longer, the oldest records are evicted immediately. Caller must be admin. Emits `max_history_size_updated`.

**Errors:** `Unauthorized`, `NotInitialized`, `InvalidConfig` (`new_max` is `0` or above `10000`)

---

### `max_history_size() → Result<u32, ExecutionError>`
Returns the history cap.

**Errors:** `NotInitialized`

---

### `get_execution_history(limit: u32) → Result<Vec<ExecutionRecord>, ExecutionError>`
Returns up to `limit` of the most recent records, newest first.

**Errors:** `NotInitialized`

---

### `execution_history_len() → Result<u32, ExecutionError>`
Returns the number of records currently stored.

**Errors:** `NotInitialized`

---

### `stats() → (u64, u64)`
Returns `(total_executions, total_errors)`. Returns `(0, 0)` before initialization.

---

### `admin() → Result<Address, ExecutionError>`
**Errors:** `NotInitialized`

---

### `transfer_admin(current, new_admin) → Result<(), ExecutionError>`
Transfers admin to `new_admin`. Emits `admin_transferred`.

**Errors:** `Unauthorized`, `NotInitialized`

---

### Types

#### `ExecutionRequest`
| Field | Type | Description |
|---|---|---|
| `target` | `Address` | Contract to invoke |
| `function` | `Symbol` | Function name |
| `simulate_first` | `bool` | Run a (real) simulation invocation first |
| `max_retries` | `u32` | Requested retries; capped at `max_retries()` |
| `args` | `Vec<Val>` | Positional arguments, already typed for the target ABI (max 20) |
| `amount` | `i128` | Transaction amount in stroops (`> 0`), used to compute `fee_paid` |

#### `ExecutionResult`
| Field | Type | Description |
|---|---|---|
| `target` | `Address` | Invoked contract |
| `function` | `Symbol` | Invoked function |
| `success` | `bool` | Always `true` (failures are returned as errors) |
| `attempts` | `u32` | Attempts used, `1` = first try |
| `simulated` | `bool` | Whether `simulate_first` was set |

#### `ExecutionRecord`
| Field | Type | Description |
|---|---|---|
| `timestamp` | `u64` | Ledger timestamp |
| `target` | `Address` | Invoked contract |
| `function` | `Symbol` | Invoked function |
| `success` | `bool` | Outcome |
| `fee_paid` | `i128` | `base_fee + resource_fee` for the request amount |

#### `SimulationResult`
| Field | Type | Description |
|---|---|---|
| `target` | `Address` | Invoked contract |
| `function` | `Symbol` | Invoked function |
| `success` | `bool` | The invocation succeeded |
| `would_fail` | `bool` | `!success` |
| `message` | `String` | `"simulation succeeded"` or `"simulation failed: transaction would be rejected"` |

#### `FeeEstimate`
| Field | Type | Description |
|---|---|---|
| `base_fee` | `i128` | Fixed `100` stroops |
| `resource_fee` | `i128` | `max(amount / 1000, 100)` stroops |
| `total_fee` | `i128` | `(base_fee + resource_fee) * surge_multiplier / 100` |
| `surge_multiplier` | `u32` | `100` (1×) or `200` (2×) |
| `high_load` | `bool` | Caller asserted load `>= 8000` bps (unverified) |

---

### `ExecutionError`

Codes are grouped by category: 1xx network (retryable), 2xx simulation (blocks execution), 3xx contract, 4xx configuration/auth.

| Error | Value | When it occurs | How to handle it |
| --- | ---: | --- | --- |
| `NetworkTimeout` | 101 | Reserved; not currently returned. | — |
| `NetworkUnavailable` | 102 | Reserved; not currently returned. | — |
| `SimulationFailed` | 201 | `execute` with `simulate_first = true` and the simulation invocation failed. | Inspect the target call and arguments; nothing further was attempted. |
| `SimulationInsufficientResources` | 202 | Reserved; not currently returned. | — |
| `ContractRejected` | 301 | Every attempt in `execute` failed. | Check the target, function and args; retry later if the failure is transient. |
| `ContractNotFound` | 302 | Reserved; not currently returned. | — |
| `ContractFunctionNotFound` | 303 | Reserved; not currently returned. | — |
| `AlreadyInitialized` | 401 | `initialize` is called a second time. | Treat as already configured. |
| `NotInitialized` | 402 | Any entry point other than `stats` runs before `initialize`. | Initialize the contract first. |
| `Unauthorized` | 403 | An admin-only function is called by a non-admin. | Sign with the admin account. |
| `InvalidConfig` | 404 | `max_retries` not in `1..=5`, `backoff_multiplier` not in `100..=10000`, or history size not in `1..=10000`. | Use a value inside the allowed range. |
| `InvalidAmount` | 405 | `amount <= 0` in `execute` or `estimate_fee`. | Pass a positive amount. |
| `ArgsTooLarge` | 406 | More than 20 `args` passed to `execute` or `simulate`. | Reduce the argument count. |

---

## Error Code Reference

Each contract defines its own `#[contracterror]` enum. Use the tables below as the canonical error-code reference for integration, monitoring, and client-side handling.

### router-core (`RouterError`)

| Error | Code | When it occurs | How to handle |
|---|---:|---|---|
| `AlreadyInitialized` | `1` | `initialize` is called after admin/state already exists | Treat as idempotent setup; skip re-initialization and proceed with existing deployment |
| `NotInitialized` | `2` | Admin-gated write methods run before first initialization | Initialize the contract first, then retry |
| `Unauthorized` | `3` | Caller is not current admin for admin-only methods | Re-submit with admin signer or transfer admin before retry |
| `RouteNotFound` | `4` | Route (or alias target) does not exist for lookup/update/remove | Register route first, or correct the route/alias name |
| `RoutePaused` | `5` | `resolve` is called for a route marked paused | Unpause that route, or route traffic to another available route |
| `RouterPaused` | `6` | Router is globally paused during `resolve`/route selection | Unpause globally before serving traffic |
| `RouteAlreadyExists` | `7` | Registering route or alias that conflicts with existing route/alias | Use a unique name, or update/remove existing entry first |
| `InvalidRouteName` | `8` | Route name is empty or whitespace-only | Validate/sanitize names client-side before submit |
| `InvalidMetadata` | `9` | Route metadata exceeds constraints (description/tags limits) | Trim metadata to allowed bounds, then retry |
| `CircularDependency` | `10` | Dependency edge would create a self-dependency or cycle | Restructure the dependency graph |
| `RouteInUse` | `11` | Removing a route that other routes depend on | Remove dependents or their dependency edges first |
| `InvalidAddress` | `12` | Registering/updating a route with the zero address | Pass a real contract address |
| `RouteExpired` | `13` | `resolve` is called for a route whose TTL ledger has been exceeded, or `extend_route_ttl` is called on an already-expired route | Re-register the route, or extend its TTL before it lapses |
| `InvalidScore` | `14` | A `RouteScore` field exceeds 100 | Keep score fields in `0..=100` |
| `InvalidTtlExtension` | `15` | TTL or TTL extension of zero ledgers | Pass a positive ledger count |
| `RecursionLimitExceeded` | `16` | Dependency graph deeper than 10 levels | Flatten the dependency graph |
| `TooManyRoutes` | `17` | Registration would exceed the `max_routes` cap | Remove routes or call `set_max_routes` |
| `InvalidScoringWeights` | `18` | Invalid `ScoringWeights` passed to `set_scoring_weights` | Use weights in `0..=1000` and `fee_divisor > 0` |

### router-registry (`RegistryError`)

| Error | Code | When it occurs | How to handle |
|---|---:|---|---|
| `AlreadyInitialized` | `1` | `initialize` called more than once | Treat as already configured; do not re-run init |
| `NotInitialized` | `2` | Admin-gated write method called before initialization | Initialize first, then retry |
| `Unauthorized` | `3` | Non-admin caller attempts register/deprecate/admin transfer | Use admin signer or perform admin handoff |
| `NotFound` | `4` | Requested `(name, version)` or constrained lookup has no match | Check name/version/constraint inputs and fallback strategy |
| `AlreadyRegistered` | `5` | `(name, version)` already exists | Bump version or update workflow instead of duplicate register |
| `AlreadyDeprecated` | `6` | Deprecating an entry that is already deprecated | Treat as idempotent deprecation and continue |
| `InvalidVersion` | `7` | Version is `0` or not strictly greater than existing versions | Submit monotonically increasing, non-zero version |
| `VersionNotFound` | `8` | Deprecating a version that does not exist | Verify version list with `versions(name)` first |
| `InvalidConstraint` | `9` | Constraint string format is invalid | Validate/normalize constraint syntax before calling |
| `AllVersionsDeprecated` | `10` | Lookup finds only deprecated versions for a name | Register a new active version or allow deprecated fallback intentionally |
| `ContractUnreachable` | `11` | `register_with_check` failed to invoke the supplied health function on the target address | Confirm the target contract exposes the allow-listed `health` or `ping` function and that it returns successfully |
| `InvalidHealthFn` | `12` | `register_with_check` rejected `health_fn` because it was not in the `health` / `ping` allow-list (issue #828) | Pass `None` or use one of `health` / `ping`; pass-through arbitrary symbols are no longer accepted to prevent state-mutating entry points being invoked on the target during registration |
| `BatchTooLarge` | `13` | `bulk_register` or `deprecate_many` called with more than 50 entries (`MAX_BULK_BATCH_SIZE`) | Split the entries into batches of at most 50 and submit each separately |

### router-access (`AccessError`)

| Error | Code | When it occurs | How to handle |
|---|---:|---|---|
| `AlreadyInitialized` | `1` | `initialize` called after super-admin already set | Treat as already initialized |
| `NotInitialized` | `2` | Operations requiring super-admin state run before init | Initialize first, then retry |
| `Unauthorized` | `3` | Caller is not super-admin or configured role admin | Use an authorized account or update role-admin assignment |
| `AlreadyHasRole` | `4` | Granting role already held (directly/inherited as implemented) | Treat as idempotent grant and continue |
| `RoleNotFound` | `5` | Revoking/removing role that is not present | Confirm membership before revoke, or treat as already removed |
| `Blacklisted` | `6` | Target/caller is blacklisted for role-management operation | Unblacklist or use a non-blacklisted account |
| `CannotBlacklistAdmin` | `7` | Attempt to blacklist current super-admin | Transfer super-admin first (if needed), then blacklist old admin |
| `DestinationAlreadyHasRole` | `8` | `transfer_role_membership` destination already holds the role | Revoke from destination first, or skip the transfer |
| `HierarchyCycle` | `9` | `set_role_parent` would create a cycle | Redesign role graph to remain acyclic |
| `InvalidExpiry` | `10` | `expires_in` overflows when added to the ledger timestamp | Use a smaller `expires_in` or `None` |
| `HierarchyTooDeep` | `11` | `set_role_parent` would exceed the maximum hierarchy depth | Flatten the role hierarchy |
| `MaxRolesExceeded` | `12` | A new role name would exceed the `max_roles` cap | Reuse an existing role or raise the cap via `set_role_limits` |
| `MaxGrantsPerRoleExceeded` | `13` | A new grant would exceed the `max_grants_per_role` cap | Revoke unused grants or raise the cap via `set_role_limits` |

### router-middleware (`MiddlewareError`)

| Error | Code | When it occurs | How to handle |
|---|---:|---|---|
| `AlreadyInitialized` | `1` | `initialize` called after admin already configured | Treat as already initialized |
| `NotInitialized` | `2` | Admin-only configuration called before initialization | Initialize first, then retry |
| `Unauthorized` | `3` | Non-admin caller tries config/global/circuit/admin updates | Use admin signer |
| `RateLimitExceeded` | `4` | `pre_call` exceeds configured per-caller route window | Back off until next window; implement retry with jitter |
| `RouteDisabled` | `5` | `pre_call` on a route configured as disabled | Re-enable route or route to alternate endpoint |
| `MiddlewareDisabled` | `6` | Global middleware toggle is disabled | Re-enable global middleware before routed calls |
| `InvalidConfig` | `7` | Invalid route config (e.g. non-zero max calls with zero window) | Validate config invariants client-side before write |
| `CircuitOpen` | `8` | Circuit breaker open for route and recovery window not elapsed | Wait for recovery window or reset circuit via admin |
| _(retired)_ | `9` | Formerly `Reentrancy`. Never returned; the `pre_call` reentrancy guard was removed. The value is reserved for compatibility with old deployments | — |

### router-timelock (`TimelockError`)

| Error | Code | When it occurs | How to handle |
|---|---:|---|---|
| `AlreadyInitialized` | `1` | `initialize` called after admin already set | Treat as already initialized |
| `NotInitialized` | `2` | Admin checks/storage reads run before initialization | Initialize first, then retry |
| `Unauthorized` | `3` | Non-admin tries queue/cancel/execute | Use timelock admin signer |
| `NotFound` | `4` | Operation ID not present in storage | Verify op ID from queue event/output before acting |
| `NotReady` | `5` | `execute` called before operation ETA | Wait until ETA then retry |
| `AlreadyExecuted` | `6` | Execute/cancel attempted on operation already executed | Treat as terminal completed state |
| `Cancelled` | `7` | Execute/cancel attempted on already-cancelled operation | Treat as terminal cancelled state |
| `DelayTooShort` | `8` | Queue delay is below configured `min_delay`, or `initialize` / `set_min_delay` is given a zero minimum delay | Submit with `delay >= min_delay`; use a non-zero minimum delay |
| `Expired` | `9` | The grace period has elapsed; the operation can no longer be executed | Queue a new operation if still needed |
| `QueueFull` | `10` | The maximum number of pending operations has been reached | Execute, cancel, or clean up pending operations first |
| `CircularDependency` | `11` | A dependency references itself or creates a cycle | Remove the cyclic dependency |
| `DependencyTooDeep` | `12` | Dependency chain exceeds maximum allowed depth | Flatten the dependency chain |
| `AlreadyQueued` | `13` | An operation with this exact (description, target, eta) already exists | Change description/target/delay or use the existing op |
| `DependencyNotExecuted` | `14` | A dependency of this operation has not yet been executed | Execute dependencies first, then retry |
| `GracePeriodTooLong` | `15` | `grace_period_seconds` exceeds `MAX_GRACE_PERIOD_SECONDS` | Use a shorter grace period |
| `ExpiryOverflow` | `16` | `eta + grace_period_seconds` overflows `u64` | Should not occur after the `GracePeriodTooLong` guard; queue a new op |
| `DelayTooLong` | `17` | `ledger_timestamp + delay` overflows `u64` | Use a smaller delay |

### router-multicall (`MulticallError`)

| Error | Code | When it occurs | How to handle |
|---|---:|---|---|
| `AlreadyInitialized` | `1` | `initialize` called after admin exists | Treat as already initialized |
| `NotInitialized` | `2` | Batch/config call before initial setup | Initialize first, then retry |
| `Unauthorized` | `3` | Non-admin calls admin-only config methods | Use admin signer for config operations |
| `BatchTooLarge` | `4` | `execute_batch` call count exceeds configured max | Split calls into smaller batches or raise max as admin |
| `EmptyBatch` | `5` | `execute_batch` called with zero calls | Validate non-empty input before submit |
| `RequiredCallFailed` | `6` | A call marked `required=true` fails and aborts batch | Retry after fixing failing target/function/input, or mark optional if acceptable |
| `InvalidConfig` | `7` | Invalid config such as `max_batch_size = 0`, or `max_total_gas` is `Some` and a call has no `instruction_budget` | Enforce positive config values client-side; declare a budget on every call when using `max_total_gas` |
| `GasLimitExceeded` | `8` | The sum of declared `instruction_budget`s exceeds `max_total_gas` | Split the batch or lower the declared budgets |

---

## Common Types

### `RouteEntry`
| Field | Type | Description |
|---|---|---|
| `address` | `Address` | Resolved contract address |
| `name` | `String` | Route name |
| `paused` | `bool` | Whether this route is paused |
| `updated_by` | `Address` | Last admin to update this route |

### `ContractEntry`
| Field | Type | Description |
|---|---|---|
| `address` | `Address` | Registered contract address |
| `name` | `String` | Human-readable name |
| `version` | `u32` | Version number |
| `deprecated` | `bool` | Whether deprecated |
| `registered_by` | `Address` | Who registered it |

### `CallDescriptor`
| Field | Type | Description |
|---|---|---|
| `target` | `Address` | Contract to call |
| `function` | `Symbol` | Function name |
| `required` | `bool` | Abort batch on failure |
| `instruction_budget` | `Option<u64>` | Reserved for future budget metering; not applied to the invocation. Required on every call when `max_total_gas` is `Some` |

### `BatchSummary`
| Field | Type | Description |
|---|---|---|
| `total` | `u32` | Total calls attempted |
| `succeeded` | `u32` | Successful calls |
| `failed` | `u32` | Failed calls |
| `budget_exceeded_count` | `u32` | Failed calls whose host error was a budget error (`ScErrorType::Budget`) |
