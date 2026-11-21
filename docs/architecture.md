# Architecture

## Overview

`stellar-router` is a modular suite of Soroban smart contracts for building composable,
upgradeable, and access-controlled multi-contract systems on Stellar. Each contract is
independently deployable and has no hard dependency on the others.

```
┌─────────────────────────────────────────────────────┐
│                    router-core                      │
│         Central dispatcher & route resolver         │
└────────────┬────────────────────────┬───────────────┘
             │                        │
    ┌────────▼────────┐      ┌────────▼────────┐
    │ router-registry │      │  router-access  │
    │ Versioned addr  │      │  Role-based ACL │
    │ book            │      │  & blacklisting │
    └─────────────────┘      └─────────────────┘
             │                        │
    ┌────────▼────────┐      ┌────────▼────────┐
    │router-middleware│      │router-timelock  │
    │ Rate limiting   │      │ Delayed change  │
    │ Call logging    │      │ execution queue │
    └─────────────────┘      └─────────────────┘
                      │
             ┌────────▼────────┐
             │router-multicall │
             │ Batch calls in  │
             │ one transaction │
             └─────────────────┘
```

Off-chain components connect to the on-chain contracts via the Soroban RPC endpoint:

```
[Stellar Network]
      │
      │  Soroban RPC
      │
[router-metrics-exporter]  ──►  Prometheus  ──►  Grafana
[api-server]               ──►  REST / WebSocket clients
```

---

## Contracts

### router-core

The entry point for all routing. Maintains a `name → address` mapping and resolves
contract addresses by route name. Supports pause controls at both the global and
per-route level.

**Key functions:**
- `initialize(admin, max_routes)` — set the admin, route capacity, and activate the contract.
- `register_route(caller, name, address)` — map a name to a contract address.
- `resolve(name) → Address` — look up the address for a route name.
- `set_route_paused(caller, name, paused)` — pause or unpause a route.
- `set_paused(caller, paused)` — global pause toggle.

**Events emitted:** `route_registered`, `routed`, `route_paused` (including the `paused` flag), `router_paused`.

---

### router-registry

A versioned address book. Each entry is keyed by `(name, version)`. Versions must
increase monotonically. Old versions can be deprecated with an optional reason string,
and `get_latest` always returns the newest non-deprecated entry.

**Key functions:**
- `register(caller, name, address, version)` — add a new versioned entry.
- `get(name, version) → ContractEntry` — fetch a specific version.
- `get_latest(name) → ContractEntry` — fetch the newest non-deprecated version.
- `get_latest_with_constraint(name, constraint)` — fetch with a version constraint (e.g. `>=2`, `<3`).
- `deprecate(caller, name, version, reason)` — mark a version deprecated with an optional reason.
- `deprecate_many(caller, entries)` — batch deprecation.

**ContractEntry fields:** `address`, `name`, `version`, `deprecated`, `deprecation_reason`, `registered_by`.

**Events emitted:** `contract_registered(name, version, None)`, `contract_deprecated(name, version, reason)`, `admin_transferred`.

---

### router-access

Role-based access control with three tiers:

| Tier | Capability |
|---|---|
| Super admin | Full control — grant/revoke any role, blacklist addresses |
| Role admin | Grant/revoke a specific named role |
| Role member | Holds a named role |

Addresses can be blacklisted to prevent them from being granted any role.

**Key functions:**
- `grant_role(caller, role, address)` / `revoke_role(caller, role, address)`
- `has_role(role, address) → bool`
- `blacklist(caller, address)` / `unblacklist(caller, address)`

---

### router-middleware

Pre/post call hooks for any route. Supports per-route rate limiting, global and
per-route enable/disable toggles, and call event logging.

**Key functions:**
- `configure_route(caller, route, max_calls_per_window, window_seconds, enabled)`
- `pre_call(caller, route)` — check rate limit and log the call attempt.
- `post_call(caller, route, success)` — log the call result.
- `enable_route(caller, route)` / `disable_route(caller, route)`

**Events emitted:** `pre_call_logged`, `post_call_logged`, `route_configured`.

---

### router-timelock

A delay queue for sensitive router changes (e.g. upgrading a registry entry).
Operations must wait a configurable minimum delay before they can be executed.
Operations can be cancelled before execution.

**Key functions:**
- `queue(proposer, description, target, delay) → op_id` — enqueue an operation.
- `execute(caller, op_id)` — execute after the ETA has passed.
- `cancel(caller, op_id)` — cancel before execution.

**States:** `Pending → Ready → Executed | Cancelled`.

---

### router-multicall

Batches multiple cross-contract calls into a single transaction. Each call can be
marked `required` (failure aborts the batch) or optional (failure is tracked but
does not abort). Returns a `BatchSummary` with success/failure counts.

`execute_batch` is a public function — any authenticated address can call it.
The admin role is only used for configuration (e.g. `set_max_batch_size`).

---

### router-execution

Execution pipeline with simulation, retries, and fee estimation. Wraps cross-contract
calls with retry logic and pre-execution simulation to estimate gas costs.

---

### router-quote

Read-only quote preview contract. Returns expected output amount, fees, exchange rate,
and price impact without executing the transaction.

**Key functions:**
- `get_quote(router_core, route_name, token_in, token_out, amount_in, fee_bps, slippage_bps, precision) → QuoteResponse`
- `get_multihop_quote(hops, amount_in, slippage_bps, precision) → QuoteResponse`
- `estimate_fee(request) → FeeEstimateResponse`

**QuoteResponse fields:** `amount_out`, `fee_amount`, `min_amount_out`, `exchange_rate`, `precision`, `price_impact_bps`.

`price_impact_bps` is calculated as `(amount_out - amount_in) * 10_000 / amount_in`.
Negative values indicate adverse price impact (user receives less than they put in).

---

## Data Flow: Typical Route Resolution

```
Caller
  │
  │  resolve("oracle")
  ▼
router-core
  │  checks: not paused globally, route not paused
  │
  │  emits: routed
  │
  └──► returns Address of oracle contract
         │
         │  Caller then invokes oracle directly
         ▼
       oracle contract
```

With middleware:

```
Caller
  │
  │  pre_call("oracle/get_price")
  ▼
router-middleware
  │  checks: route enabled, rate limit not exceeded
  │  emits: pre_call_logged
  │
  ▼
Caller invokes oracle
  │
  │  post_call("oracle/get_price", success=true)
  ▼
router-middleware
  │  emits: post_call_logged
```

With timelock (for sensitive config changes):

```
Admin
  │  queue("upgrade oracle to v2", target=new_addr, delay=86400)
  ▼
router-timelock  ──►  stores op with eta = now + delay
  │
  │  (24 hours pass)
  │
Admin
  │  execute(op_id)
  ▼
router-timelock  ──►  executes the queued operation
```

---

## Off-Chain Components

### router-metrics-exporter

An off-chain Rust binary that polls the Soroban RPC endpoint and exposes contract
metrics in Prometheus format. Grafana dashboards are provided for visualization.

```
router-metrics-exporter
  │  polls Soroban RPC every N seconds
  │  reads ledger entries and event streams
  │
  ├──► /metrics  (Prometheus scrape endpoint)
  │
Prometheus  ──►  Grafana (dashboards + alerts)
```

Configuration via environment variables:
- `ROUTER_RPC_URL` — Soroban RPC endpoint URL.
- `ROUTER_CORE_CONTRACT_ID` — deployed router-core contract ID.
- `ROUTER_MIDDLEWARE_CONTRACT_ID` — deployed router-middleware contract ID.
- `ROUTER_REGISTRY_CONTRACT_ID` — deployed router-registry contract ID.

See [`metrics/README.md`](../metrics/README.md) for full setup instructions.

### api-server

A REST and WebSocket API server that proxies Soroban RPC calls and provides a
higher-level interface for off-chain clients. Supports route resolution, quote
fetching, and event streaming.

---

## Deployment Order

Contracts have no hard on-chain dependencies, but the recommended deployment order
ensures that router-core is available before contracts that optionally integrate with it:

1. `router-registry`
2. `router-access`
3. `router-middleware`
4. `router-timelock`
5. `router-multicall`
6. `router-core`
7. `router-execution` (optionally references router-core)
8. `router-quote` (optionally references router-core for route resolution)

See [`docs/deployment.md`](deployment.md) for full deployment instructions.

---

## Storage Layout (Issue #1633)

All nine contracts currently store every key — including data that grows with
usage — under `env.storage().instance()`. Instance storage is a single ledger
entry per contract, read on every invocation, and bounded by the network's
contract-data entry size limit (64 KiB today). A contract whose instance
entry grows past that limit fails **every** call, including admin calls that
could otherwise clean it up — there is no recovery path short of a migration
to a new contract instance.

The existing per-key caps (`max_routes`, `MAX_TRACKED_ROUTES`, history size
limits, etc.) were sized to bound unbounded growth, not to fit the 64 KiB
ledger-entry limit — several of them are far above what the limit allows at
typical encoded key/value sizes. See #1632 for one concrete case
(`router-middleware`'s call log) that reproduces this directly.

### Per-contract storage key classification

| Contract | Stays in `instance()` (small, bounded, read on every call) | Moves to `persistent()` (grows with usage) | Moves to `temporary()` (naturally expires) |
|---|---|---|---|
| `router-core` | `Admin`, global config/flags, counters | `Route(name)`, aliases, tags, dependencies, scores | — |
| `router-registry` | `Admin`, global config | Every registered name/version entry, the address index | — |
| `router-access` | `Admin`, role definitions (bounded, admin-managed) | `AddressRoles`, `RoleMembers`, blacklist entries | `RoleExpiry` (already has a natural expiry semantic) |
| `router-middleware` | `Admin`, `GlobalEnabled`, `TotalCalls`, `ConfiguredRoutes` | `RouteCallState` (per-caller rate-limit map), `CallLog`/`CallLogSummary` (see #1632 for the concrete redesign) | Per-caller rate-limit window entries, once split out of `RouteCallState` |
| `router-timelock` | `Admin`, config | Queued operations | Executed/cancelled operations, once past their retention window |
| `router-execution` | `Admin`, `TotalExecutions`, `TotalErrors`, config | `ExecHistory` (see #1626, already moved to a persistent ring buffer) | — |
| `router-multicall` | `Admin`, config, `TotalBatches` | Stored batch results (`DataKey::BatchResult`) | — |
| `router-quote` | `Admin`, `ConfiguredRoutes` index, default fee | Per-route `RouteFee` / `RouteFeeTiers` (currently small enough individually, but the index itself should stay bounded by `MAX_TRACKED_ROUTES`) | — |
| `router-common` | n/a (shared library crate, no storage of its own) | n/a | n/a |

Each `persistent()` entry needs its own TTL extension (via
`extend_instance_ttl`'s persistent-storage counterpart) on every
read/write that touches it, following the same threshold/extend-to pattern
already used for instance storage — see `router_common::extend_instance_ttl`
and its call sites across every contract for the convention to mirror.
`temporary()` entries don't need TTL management at all; they're simply gone
once their ledger-defined temporary-entry lifetime elapses, which is the
right fit for rate-limit windows and similar short-lived bookkeeping.

### Status

This issue tracks the overall plan; #1626 and #1632 track (or, in #1626's
case, have already landed) the concrete per-contract migrations for
`router-execution`'s execution history and `router-middleware`'s call log
respectively. The remaining contracts' persistent-storage migrations are not
yet implemented — this section exists so each migration PR has an agreed
target layout to implement against, rather than ad hoc per-contract
decisions. A migration note belongs in
[`docs/upgrading.md`](upgrading.md) once a given contract's migration lands,
since moving existing instance-storage data to persistent storage changes
how that data is read and (for already-deployed instances) requires a
one-time migration step.
