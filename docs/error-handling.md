# Error Handling Guide

This guide provides a comprehensive overview of error handling across the stellar-router contract suite. It explains the error hierarchy, per-contract error codes, best practices for handling errors in cross-contract calls, and distinguishes between retryable and terminal errors.

## Table of Contents

- [Error Hierarchy Overview](#error-hierarchy-overview)
- [Per-Contract Error Tables](#per-contract-error-tables)
- [Error Handling Best Practices](#error-handling-best-practices)
- [Retryable vs Terminal Errors](#retryable-vs-terminal-errors)
- [Error Propagation in Multicall](#error-propagation-in-multicall)

## Error Hierarchy Overview

The stellar-router suite follows a consistent error pattern across all contracts. Errors are organized into logical categories with numeric codes that indicate their nature and severity.

### Common Error Patterns

Most contracts share these foundational error codes:

| Code | Error Name | Description |
|------|------------|-------------|
| 1 | `AlreadyInitialized` | The contract has already been initialized and cannot be initialized again. |
| 2 | `NotInitialized` | The contract has not been initialized. Call `initialize()` first. |
| 3 | `Unauthorized` | The caller does not have permission to perform this action. |

### Contract-Specific Error Categories

Beyond the common errors, each contract defines domain-specific error codes:

- **router-access**: Role-based access control errors (4-7)
- **router-core**: Route management errors (4-14)
- **router-registry**: Contract registration and versioning errors (4-11)
- **router-middleware**: Rate limiting and circuit breaker errors (4-8)
- **router-execution**: Structured error hierarchy with categories (101-405)
- **router-quote**: Fee calculation and quote errors (4-8)
- **router-multicall**: Batch execution errors (4-8)
- **router-timelock**: Time-locked operation errors (4-11)

### Special Case: router-execution Error Categories

The `router-execution` contract uses a unique hierarchical error system with category prefixes:

- **Network errors (1xx)**: Transient connectivity issues - retryable
- **Simulation errors (2xx)**: Pre-execution validation failures - blocks execution
- **Contract errors (3xx)**: On-chain contract rejections - non-retryable
- **Config errors (4xx)**: Misconfiguration or unauthorized access - terminal

This pattern allows callers to quickly determine error severity and appropriate recovery actions based on the error code range.

## Per-Contract Error Tables

### router-access

Role-based access control for the stellar-router suite.

| Code | Error Name | Meaning | Recovery Action |
|------|------------|---------|-----------------|
| 1 | `AlreadyInitialized` | Contract already initialized | No action needed; initialization is one-time |
| 2 | `NotInitialized` | Contract not initialized | Call `initialize()` with a super-admin address |
| 3 | `Unauthorized` | Caller lacks permission | Verify caller has required role or is admin |
| 4 | `AlreadyHasRole` | Address already has the role | Revoke existing role first if re-granting is needed |
| 5 | `RoleNotFound` | Role not found for the address | Check role name spelling or grant the role first |
| 6 | `Blacklisted` | Address is blacklisted | Remove from blacklist via `unblacklist()` if appropriate |
| 7 | `CannotBlacklistAdmin` | Cannot blacklist the super-admin | This is a safety guard; choose a different target |

### router-core

Core routing logic and route management.

| Code | Error Name | Meaning | Recovery Action |
|------|------------|---------|-----------------|
| 1 | `AlreadyInitialized` | Contract already initialized | No action needed |
| 2 | `NotInitialized` | Contract not initialized | Call `initialize()` with admin and registry address |
| 3 | `Unauthorized` | Caller lacks permission | Verify caller is the admin |
| 4 | `RouteNotFound` | Route does not exist | Register the route via `register_route()` |
| 5 | `RoutePaused` | Route is paused | Unpause via `set_route_paused()` if appropriate |
| 6 | `RouterPaused` | Router is globally paused | Unpause via `set_router_paused()` if appropriate |
| 7 | `RouteAlreadyExists` | Route already registered | Use a different route name or remove existing route |
| 8 | `InvalidRouteName` | Route name is invalid (empty or whitespace) | Provide a non-empty, non-whitespace route name |
| 9 | `InvalidMetadata` | Metadata is invalid | Check metadata format and constraints |
| 10 | `CircularDependency` | Route depends on itself creating a cycle | Restructure route dependencies to break the cycle |
| 11 | `RouteInUse` | Route is in use and cannot be modified | Wait for route to become available or remove dependencies |
| 12 | `InvalidAddress` | Provided address is invalid | Verify the address is a valid Stellar address |
| 13 | `RouteExpired` | Route has expired | Re-register the route with a new expiry time |
| 14 | `InvalidScore` | Route score is invalid | Provide a valid score within allowed range |

### router-registry

Contract address registry with version management.

| Code | Error Name | Meaning | Recovery Action |
|------|------------|---------|-----------------|
| 1 | `AlreadyInitialized` | Contract already initialized | No action needed |
| 2 | `NotInitialized` | Contract not initialized | Call `initialize()` with admin address |
| 3 | `Unauthorized` | Caller lacks permission | Verify caller is the admin |
| 4 | `NotFound` | Contract or version not found | Register the contract or verify version exists |
| 5 | `AlreadyRegistered` | Contract already registered | Use `update()` instead or remove existing registration |
| 6 | `AlreadyDeprecated` | Contract already deprecated | Cannot deprecate an already deprecated contract |
| 7 | `InvalidVersion` | Version format is invalid | Use valid version format (e.g., semantic versioning) |
| 8 | `VersionNotFound` | Specified version does not exist | Register the version or query available versions |
| 9 | `InvalidConstraint` | Constraint is invalid | Verify constraint format and values. |
| 10 | `AllVersionsDeprecated` | All versions are deprecated | Register a new active version |
| 11 | `ContractUnreachable` | Contract cannot be reached | Verify contract address and deployment status |

### router-middleware

Pre/post call hooks with rate limiting and circuit breaking.

| Code | Error Name | Meaning | Recovery Action |
|------|------------|---------|-----------------|
| 1 | `AlreadyInitialized` | Contract already initialized | No action needed |
| 2 | `NotInitialized` | Contract not initialized | Call `initialize()` with admin address |
| 3 | `Unauthorized` | Caller lacks permission | Verify caller is the admin |
| 4 | `RateLimitExceeded` | Caller exceeded rate limit | Wait for rate limit window to reset or increase limit |
| 5 | `RouteDisabled` | Route is disabled | Enable route via `configure_route()` if appropriate |
| 6 | `MiddlewareDisabled` | Middleware globally disabled | Enable via `set_global_enabled()` if appropriate |
| 7 | `InvalidConfig` | Configuration is invalid | Verify configuration parameters (e.g., window_seconds > 0 when max_calls > 0) |
| 8 | `CircuitOpen` | Circuit breaker is open for route | Wait for recovery window or manually close circuit |

### router-execution

Transaction execution pipeline with structured error handling.

| Code | Error Name | Meaning | Recovery Action |
|------|------------|---------|-----------------|
| 101 | `NetworkTimeout` | RPC node did not respond | Retry with exponential backoff |
| 102 | `NetworkUnavailable` | Network connectivity issue | Retry with exponential backoff |
| 201 | `SimulationFailed` | Simulation detected transaction would fail | Fix transaction parameters or logic before retrying |
| 202 | `SimulationInsufficientResources` | Insufficient resources (budget/fees) | Increase fee budget or reduce transaction complexity |
| 301 | `ContractRejected` | Target contract rejected the call | Check contract logic and parameters; do not retry without changes |
| 302 | `ContractNotFound` | Target contract not found at address | Verify contract address and deployment |
| 303 | `ContractFunctionNotFound` | Function does not exist on target contract | Verify function name and signature |
| 401 | `AlreadyInitialized` | Contract already initialized | No action needed |
| 402 | `NotInitialized` | Contract not initialized | Call `initialize()` with admin and config |
| 403 | `Unauthorized` | Caller lacks permission | Verify caller is the admin |
| 404 | `InvalidConfig` | Configuration is invalid | Verify configuration parameters (e.g., max_retries ≤ 5, backoff_multiplier 100-10000) |
| 405 | `InvalidAmount` | Amount is invalid (≤ 0) | Provide a positive amount |

### router-quote

Quote calculation and route comparison.

| Code | Error Name | Meaning | Recovery Action |
|------|------------|---------|-----------------|
| 1 | `AlreadyInitialized` | Contract already initialized | No action needed |
| 2 | `NotInitialized` | Contract not initialized | Call `initialize()` with admin and default fee |
| 3 | `Unauthorized` | Caller lacks permission | Verify caller is the admin |
| 4 | `InvalidAmount` | Amount is invalid (≤ 0) | Provide a positive amount |
| 5 | `InvalidFeeBps` | Fee basis points > 10000 | Provide fee_bps ≤ 10000 (100%) |
| 6 | `NoQuotesProvided` | Empty quotes vector | Provide at least one quote request |
| 7 | `RouteNotFound` | Route not found | Register route fee or use default fee |
| 8 | `ArithmeticOverflow` | Calculation overflow | Reduce amount or fee to prevent overflow |

### router-multicall

Batch multiple cross-contract read calls in a single transaction.

| Code | Error Name | Meaning | Recovery Action |
|------|------------|---------|-----------------|
| 1 | `AlreadyInitialized` | Contract already initialized | No action needed |
| 2 | `NotInitialized` | Contract not initialized | Call `initialize()` with admin and max_batch_size |
| 3 | `Unauthorized` | Caller lacks permission | Verify caller is the admin for config operations |
| 4 | `BatchTooLarge` | Batch exceeds max_batch_size | Reduce batch size or increase max_batch_size |
| 5 | `EmptyBatch` | Batch is empty | Provide at least one call descriptor |
| 6 | `RequiredCallFailed` | A required call failed | Fix the failing call or mark it as optional |
| 7 | `InvalidConfig` | Configuration is invalid | Verify configuration (e.g., max_batch_size > 0) |
| 8 | `Reentrancy` | Reentrancy detected | This is a safety guard; do not retry |

### router-timelock

Delayed execution queue for sensitive configuration changes.

| Code | Error Name | Meaning | Recovery Action |
|------|------------|---------|-----------------|
| 1 | `AlreadyInitialized` | Contract already initialized | No action needed |
| 2 | `NotInitialized` | Contract not initialized | Call `initialize()` with admin, min_delay, and max_pending_ops |
| 3 | `Unauthorized` | Caller lacks permission | Verify caller is the admin |
| 4 | `NotFound` | Operation not found | Verify operation ID |
| 5 | `NotReady` | Operation ETA has not elapsed | Wait until ETA elapses before executing |
| 6 | `AlreadyExecuted` | Operation already executed | Cannot execute an operation twice |
| 7 | `Cancelled` | Operation was cancelled | Cannot execute a cancelled operation |
| 8 | `DelayTooShort` | Delay is less than min_delay | Use a delay ≥ min_delay |
| 9 | `Expired` | Operation grace period has elapsed | Queue a new operation with fresh ETA |
| 10 | `QueueFull` | Maximum pending operations reached | Wait for operations to execute or increase max_pending_ops |
| 10 | `CircularDependency` | Operation depends on itself | Restructure dependencies to break the cycle |
| 11 | `DependencyTooDeep` | Dependency chain too deep | Simplify dependency chain |

## Error Handling Best Practices

### Cross-Contract Call Error Handling

When making cross-contract calls within the stellar-router suite, follow these guidelines:

1. **Always check for `NotInitialized` first**
   - Before calling any contract function, verify the contract has been initialized
   - This prevents confusing errors that mask the root cause

2. **Handle authorization errors gracefully**
   - `Unauthorized` errors indicate a permission problem, not a system failure
   - Log the unauthorized access attempt for security monitoring
   - Do not retry authorization failures without changing the caller

3. **Use batch operations with appropriate failure modes**
   - For `router-multicall`, set `required=false` for non-critical calls
   - Use `fail_fast=true` to stop processing on first failure if appropriate
   - Inspect `BatchCallResult` to identify which calls succeeded/failed

4. **Validate inputs before making calls**
   - Check for empty strings, zero amounts, and invalid addresses locally
   - This prevents unnecessary contract calls and gas waste

5. **Implement exponential backoff for retryable errors**
   - Network errors (router-execution 101-102) should be retried with backoff
   - Rate limit errors (router-middleware 4) should respect the configured window
   - Circuit breaker errors (router-middleware 8) should wait for recovery window

### Error Message Parsing

Each contract provides an error message helper function that converts error codes to human-readable strings:

- `router-access`: `access_error_message()`
- `router-core`: `router_error_message()`
- `router-registry`: `registry_error_message()`
- `router-middleware`: (implicit via contract error enum)
- `router-execution`: (implicit via contract error enum)
- `router-quote`: (implicit via contract error enum)
- `router-multicall`: (implicit via contract error enum)
- `router-timelock`: (implicit via contract error enum)

Use these helpers when logging errors or displaying them to users.

### Event-Based Error Monitoring

All contracts emit events for error conditions. Monitor these events for:

- `execution_error` (router-execution): Tracks execution failures with attempt counts
- `call_failed` (router-multicall): Indicates which call in a batch failed
- `circuit_opened` (router-middleware): Circuit breaker activation
- `rate_limit_exceeded` (router-middleware): Rate limit violations

Events provide structured data for off-chain monitoring and alerting.

## Retryable vs Terminal Errors

Understanding which errors are worth retrying is critical for building robust integrations.

### Retryable Errors

These errors indicate transient conditions that may resolve with time:

| Error | Contract | Retry Strategy |
|-------|----------|----------------|
| `NetworkTimeout` (101) | router-execution | Retry with exponential backoff |
| `NetworkUnavailable` (102) | router-execution | Retry with exponential backoff |
| `RateLimitExceeded` (4) | router-middleware | Wait for rate limit window to reset |
| `CircuitOpen` (8) | router-middleware | Wait for recovery window to elapse |
| `QueueFull` (10) | router-timelock | Wait for operations to execute |

**Retry Guidelines:**
- Use exponential backoff for network errors
- Respect configured time windows for rate limits and circuit breakers
- Set a maximum retry count (router-execution defaults to 5)
- Log retry attempts for debugging

### Terminal Errors

These errors indicate permanent failures that will not succeed without changes:

| Error | Contract | Why Terminal |
|-------|----------|--------------|
| `AlreadyInitialized` (1) | All contracts | Initialization is one-time |
| `NotInitialized` (2) | All contracts | Must initialize before any operation |
| `Unauthorized` (3) | All contracts | Permission cannot be gained by retrying |
| `InvalidConfig` (7) | router-middleware, router-execution, router-multicall | Configuration must be corrected |
| `InvalidAmount` (4, 405) | router-quote, router-execution | Input must be corrected |
| `RouteNotFound` (4) | router-core | Route must be registered first |
| `ContractRejected` (301) | router-execution | Contract logic rejected the call |
| `ContractNotFound` (302) | router-execution | Contract address is wrong |
| `AlreadyExecuted` (6) | router-timelock | Operations cannot execute twice |
| `Cancelled` (7) | router-timelock | Cancelled operations cannot execute |

**Terminal Error Handling:**
- Do NOT retry these errors
- Correct the underlying condition (e.g., initialize contract, fix permissions)
- Alert administrators if configuration errors occur in production
- Validate inputs locally to prevent terminal errors

### Conditional Retry Errors

Some errors may be retryable depending on context:

| Error | Contract | When Retryable |
|-------|----------|----------------|
| `SimulationFailed` (201) | router-execution | Only if transaction parameters change |
| `SimulationInsufficientResources` (202) | router-execution | Only if fee budget increases |
| `RoutePaused` (5) | router-core | If route is unpaused by admin |
| `RouterPaused` (6) | router-core | If router is unpaused by admin |
| `RouteDisabled` (5) | router-middleware | If route is enabled by admin |
| `MiddlewareDisabled` (6) | router-middleware | If middleware is enabled by admin |

## Error Propagation in Multicall

The `router-multicall` contract provides structured error reporting for batch operations.

### BatchCallResult Structure

When calling `execute_batch()`, the result includes:

```rust
pub struct BatchCallResult {
    pub successes: Vec<BatchCallSuccess>,  // Successful calls with indices
    pub failures: Vec<BatchFailure>,      // Failed calls with messages
}
```

Each entry includes:
- **Index**: Position in the original calls array
- **Success/Failure**: Outcome of the call
- **Message**: Human-readable error message for failures

### Failure Modes

1. **Fail-Fast Mode** (`fail_fast=true`)
   - Stops processing on first failure
   - Returns partial results up to the failure point
   - Useful when subsequent calls depend on earlier ones

2. **Continue on Error** (`fail_fast=false`)
   - Processes all calls regardless of failures
   - Returns complete success/failure mapping
   - Useful for independent calls where partial success is acceptable

3. **Required Calls** (`required=true`)
   - If a required call fails, the entire batch aborts
   - Returns `RequiredCallFailed` error
   - Use for critical operations that must succeed

### Error Messages in Batch Results

Failed calls include descriptive messages:

- `"budget_exceeded"` - Call failed and had an instruction_budget set
- `"invoke_failed"` - Contract invocation failed for other reasons

These messages help identify the root cause of batch failures without exposing internal contract details.

### Example: Handling Batch Errors

```rust
let result = multicall.execute_batch(
    &env,
    &caller,
    calls,
    false,  // simulate
    true,   // store_results
    false,  // fail_fast
)?;

// Check for failures
for failure in result.failures.iter() {
    let call = calls.get(failure.index as u32).unwrap();
    match failure.message.as_str() {
        "budget_exceeded" => {
            // Handle budget exceeded for this specific call
        }
        "invoke_failed" => {
            // Handle general invocation failure
        }
        _ => {
            // Handle unknown failure
        }
    }
}
```

### Async Result Inspection

When `store_results=true`, individual call results are persisted under `DataKey::BatchResult(batch_id, call_index)`. Use:

- `get_batch_result(batch_id, call_index)` - Retrieve a single result
- `get_batch_results(batch_id)` - Retrieve all results for a batch

This allows async inspection of batch execution results for monitoring and debugging.
