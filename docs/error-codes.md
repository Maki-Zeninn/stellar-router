# Error Code Registry

Centralized reference for all error discriminant values across the stellar-router contract suite.
Use this document to avoid collisions when adding new error variants.

## Known Collisions

None. Every contract's error enum currently uses unique discriminant values.

> If a collision is ever introduced, record it here until it is resolved.
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
| 10   | `CircularDependency` |
| 11   | `RouteInUse` |
| 12   | `InvalidAddress` |
| 13   | `RouteExpired` |
| 14   | `InvalidScore` |
| 15   | `InvalidTtlExtension` |
| 16   | `RecursionLimitExceeded` |
| 17   | `TooManyRoutes` |
| 18   | `InvalidScoringWeights` |
| **19** | ← next available |

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
| 10   | `QueueFull` |
| 11   | `CircularDependency` |
| 12   | `DependencyTooDeep` |
| 13   | `AlreadyQueued` |
| 14   | `DependencyNotExecuted` |
| 15   | `GracePeriodTooLong` |
| 16   | `ExpiryOverflow` |
| 17   | `DelayTooLong` |
| **18** | ← next available |

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
| 401  | `AlreadyInitialized` | Config/auth |
| 402  | `NotInitialized` | Config/auth |
| 403  | `Unauthorized` | Config/auth |
| 404  | `InvalidConfig` | Config/auth |
| 405  | `InvalidAmount` | Config/auth |
| 406  | `ArgsTooLarge` | Config/auth |
| **103** | ← next network code |
| **203** | ← next simulation code |
| **304** | ← next contract code |
| **407** | ← next config/auth code |

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
| 9    | `GasLimitExceeded` |
| 10   | `ArgsTooLarge` |
| **11** | ← next available |

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
| 7    | `RouteNotFound` *(retired; do not reuse)* |
| 8    | `ArithmeticOverflow` |
| 9    | `TooManyRoutes` |
| 10   | `InvalidFeeTier` |
| 11   | `TooManyTiers` |
| **12** | ← next available |

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
| 9    | `Reentrancy` *(retired; no longer returned; do not reuse)* |
| **10** | ← next available |

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
| 12   | `InvalidHealthFn` |
| 13   | `BatchTooLarge` |
| **14** | ← next available |

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
| 8    | `DestinationAlreadyHasRole` |
| 9    | `HierarchyCycle` |
| 10   | `InvalidExpiry` |
| 11   | `HierarchyTooDeep` |
| 12   | `MaxRolesExceeded` |
| 13   | `MaxGrantsPerRoleExceeded` |
| **14** | ← next available |

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

5. **Resolve any known collision** listed at the top of this document before the next
   mainnet deployment by reassigning one of the duplicate variants to the "next
   available" code and publishing a migration note.

6. **router-execution uses ranges** — continue that convention. The `4xx` range is already in use for config/auth errors:
   - `1xx` for network/transient errors
   - `2xx` for simulation errors
   - `3xx` for contract-call errors
   - `4xx` for config/auth errors
