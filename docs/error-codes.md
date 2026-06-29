# Error Code Registry

Centralized reference for all error discriminant values across the stellar-router contract suite.
Use this document to avoid collisions when adding new error variants.

## Known Collisions

| Code | Contracts / Variants |
|------|---------------------|
| 10   | `RouterError::CircularDependency` and `RouterError::InvalidScore` (router-core) |
| 10   | `TimelockError::QueueFull` and `TimelockError::CircularDependency` (router-timelock) |

> These collisions exist in the current codebase and are tracked here for visibility.
> See the per-contract tables below for authoritative values. New variants **must not**
> reuse any code listed in this file.

---

## router-core — `RouterError`

| Code | Variant |
|------|---------|
| 1    | `AlreadyInitialized` |
| 2    | `NotInitialized` |
| 3    | `Unauthorized` |
| 4    | `RouteNotFound` |
| 5    | `RoutePaused` |
| 6    | `RouterPaused` |
| 7    | `RouteAlreadyExists` |
| 8    | `InvalidRouteName` |
| 9    | `InvalidMetadata` |
| 10   | `CircularDependency` ⚠️ collision with `InvalidScore` |
| 10   | `InvalidScore` ⚠️ collision with `CircularDependency` |
| 11   | `RouteInUse` |
| 12   | `InvalidAddress` |
| 13   | `RouteExpired` |
| **14** | ← next available |

---

## router-timelock — `TimelockError`

| Code | Variant |
|------|---------|
| 1    | `AlreadyInitialized` |
| 2    | `NotInitialized` |
| 3    | `Unauthorized` |
| 4    | `NotFound` |
| 5    | `NotReady` |
| 6    | `AlreadyExecuted` |
| 7    | `Cancelled` |
| 8    | `DelayTooShort` |
| 9    | `Expired` |
| 10   | `QueueFull` ⚠️ collision with `CircularDependency` |
| 10   | `CircularDependency` ⚠️ collision with `QueueFull` |
| 11   | `DependencyTooDeep` |
| **12** | ← next available |

---

## router-execution — `ExecutionError`

Uses range-based codes to distinguish error categories.

| Code | Variant | Category |
|------|---------|----------|
| 101  | `NetworkTimeout` | Network (transient) |
| 102  | `NetworkUnavailable` | Network (transient) |
| 201  | `SimulationFailed` | Simulation |
| 202  | `SimulationInsufficientResources` | Simulation |
| 301  | `ContractRejected` | Contract |
| 302  | `ContractNotFound` | Contract |
| 303  | `ContractFunctionNotFound` | Contract |
| **103** | ← next network code |
| **203** | ← next simulation code |
| **304** | ← next contract code |

---

## router-multicall — `MulticallError`

| Code | Variant |
|------|---------|
| 1    | `AlreadyInitialized` |
| 2    | `NotInitialized` |
| 3    | `Unauthorized` |
| 4    | `BatchTooLarge` |
| 5    | `EmptyBatch` |
| 6    | `RequiredCallFailed` |
| 7    | `InvalidConfig` |
| 8    | `Reentrancy` |
| **9** | ← next available |

---

## router-quote — `QuoteError`

| Code | Variant |
|------|---------|
| 1    | `AlreadyInitialized` |
| 2    | `NotInitialized` |
| 3    | `Unauthorized` |
| 4    | `InvalidAmount` |
| 5    | `InvalidFeeBps` |
| 6    | `NoQuotesProvided` |
| 7    | `RouteNotFound` |
| 8    | `ArithmeticOverflow` |
| **9** | ← next available |

---

## router-middleware — `MiddlewareError`

| Code | Variant |
|------|---------|
| 1    | `AlreadyInitialized` |
| 2    | `NotInitialized` |
| 3    | `Unauthorized` |
| 4    | `RateLimitExceeded` |
| 5    | `RouteDisabled` |
| 6    | `MiddlewareDisabled` |
| 7    | `InvalidConfig` |
| 8    | `CircuitOpen` |
| **9** | ← next available |

---

## router-registry — `RegistryError`

| Code | Variant |
|------|---------|
| 1    | `AlreadyInitialized` |
| 2    | `NotInitialized` |
| 3    | `Unauthorized` |
| 4    | `NotFound` |
| 5    | `AlreadyRegistered` |
| 6    | `AlreadyDeprecated` |
| 7    | `InvalidVersion` |
| 8    | `VersionNotFound` |
| 9    | `InvalidConstraint` |
| 10   | `AllVersionsDeprecated` |
| 11   | `ContractUnreachable` |
| **12** | ← next available |

---

## router-access — `AccessError`

| Code | Variant |
|------|---------|
| 1    | `AlreadyInitialized` |
| 2    | `NotInitialized` |
| 3    | `Unauthorized` |
| 4    | `AlreadyHasRole` |
| 5    | `RoleNotFound` |
| 6    | `Blacklisted` |
| 7    | `CannotBlacklistAdmin` |
| **8** | ← next available |

---

## Contributor Guidelines

1. **Check this file before adding a new error variant.** Pick the next available code
   for the contract you are modifying.

2. **Update this file in the same PR** that adds the new variant. Keep the "next
   available" marker up to date.

3. **Never reuse a code value**, even if the old variant is removed. Discriminant
   values are part of the on-chain ABI and removing them is a breaking change.

4. **Do not add a new variant with the same code as an existing variant.** Rust allows
   duplicate discriminant values, but on-chain they produce ambiguous error codes.

5. **Resolve the known collisions** listed at the top of this document before the next
   mainnet deployment by reassigning one of the duplicate variants to the "next
   available" code and publishing a migration note.

6. **router-execution uses ranges** — continue that convention:
   - `1xx` for network/transient errors
   - `2xx` for simulation errors
   - `3xx` for contract-call errors
   - Add a new range (e.g. `4xx`) for entirely new error categories
