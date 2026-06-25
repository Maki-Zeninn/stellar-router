# Error Handling Guide

This document describes the error handling strategy used across the Stellar Router contract suite. It provides a reference for error codes, explains how errors propagate between contracts, and outlines best practices for callers integrating with the protocol.

## Overview

Contracts in the router suite use typed error enums represented as numeric values. Errors are designed to be:

- Deterministic
- Machine-readable
- Consistent across contracts
- Safe to propagate across cross-contract calls

Consumers should avoid relying on error message strings and instead use the numeric error codes and variants documented below.

---

# Common Error Patterns

Several contracts share common error categories.

| Error | Meaning | Typical Recovery |
|---------|---------|---------|
| AlreadyInitialized | Contract initialization was attempted more than once | Do not retry |
| NotInitialized | Contract has not been initialized yet | Initialize first |
| Unauthorized | Caller lacks required permissions | Use an authorized account |
| InvalidInput | Provided arguments failed validation | Correct request data |
| NotFound | Requested resource does not exist | Verify identifiers |
| DuplicateEntry | Entry already exists | Avoid duplicate creation |
| ExecutionFailed | Underlying operation failed | Investigate cause |
| ContractCallFailed | Cross-contract invocation failed | Inspect propagated error |

---

# Retryable vs Terminal Errors

## Retryable Errors

Retryable errors indicate a transient condition that may succeed if attempted later.

Examples include:

| Error | Reason |
|---------|---------|
| NetworkTimeout | Temporary network issue |
| NetworkUnavailable | Dependency temporarily unavailable |
| RateLimitExceeded | Request quota exceeded |
| NotReady | Resource not ready yet |

### Recommended Handling

1. Retry with exponential backoff.
2. Limit maximum retry attempts.
3. Log repeated failures.

---

## Terminal Errors

Terminal errors require corrective action and should not be retried automatically.

Examples include:

| Error | Reason |
|---------|---------|
| Unauthorized | Permission issue |
| AlreadyInitialized | Invalid state transition |
| InvalidInput | Request validation failure |
| NotFound | Missing resource |
| DuplicateEntry | Existing resource conflict |

### Recommended Handling

1. Surface error to user.
2. Correct request or permissions.
3. Submit a new transaction if appropriate.

---

# Router Registry Errors

The registry contract manages protocol configuration and contract discovery.

| Code | Error | Meaning | Recovery |
|---------|---------|---------|---------|
| 1 | AlreadyInitialized | Contract already initialized | Do not retry |
| 2 | NotInitialized | Contract not initialized | Initialize first |
| 3 | Unauthorized | Caller lacks privileges | Use admin account |
| 4 | EntryNotFound | Registry entry missing | Verify identifier |
| 5 | DuplicateEntry | Entry already exists | Avoid duplicate registration |

---

# Router Core Errors

The core router coordinates protocol execution.

| Code | Error | Meaning | Recovery |
|---------|---------|---------|---------|
| 1 | Unauthorized | Missing required permissions | Use authorized account |
| 2 | InvalidRoute | Route configuration invalid | Correct route data |
| 3 | ExecutionFailed | Route execution failed | Inspect underlying failure |
| 4 | NotInitialized | Contract not initialized | Initialize contract |

---

# Router Execution Errors

The execution contract performs routed operations.

| Code | Error | Meaning | Recovery |
|---------|---------|---------|---------|
| 1 | Unauthorized | Missing permission | Use authorized caller |
| 2 | InvalidExecution | Execution parameters invalid | Correct parameters |
| 3 | ExecutionFailed | Execution failed | Inspect underlying cause |
| 4 | DependencyFailure | External dependency failed | Retry if transient |

---

# Router Multicall Errors

The multicall contract executes multiple operations in a single transaction.

| Code | Error | Meaning | Recovery |
|---------|---------|---------|---------|
| 1 | RequiredCallFailed | Mandatory call failed | Inspect underlying error |
| 2 | InvalidBatch | Batch request invalid | Fix batch structure |
| 3 | Unauthorized | Caller unauthorized | Use correct account |
| 4 | ExecutionFailed | Batch execution failed | Investigate failure |

---

# Router Middleware Errors

Middleware contracts perform validation and execution checks.

| Code | Error | Meaning | Recovery |
|---------|---------|---------|---------|
| 1 | Unauthorized | Caller unauthorized | Use authorized account |
| 2 | ValidationFailed | Middleware validation failed | Correct request |
| 3 | DependencyFailure | Upstream dependency failed | Retry if transient |

---

# Router Access Errors

Access contracts manage permissions and authorization.

| Code | Error | Meaning | Recovery |
|---------|---------|---------|---------|
| 1 | Unauthorized | Missing required role | Obtain required permissions |
| 2 | RoleNotFound | Role does not exist | Verify role identifier |
| 3 | DuplicateRole | Role already exists | Avoid duplicate creation |

---

# Router Timelock Errors

Timelock contracts enforce delayed execution.

| Code | Error | Meaning | Recovery |
|---------|---------|---------|---------|
| 1 | NotReady | Delay period not complete | Retry later |
| 2 | AlreadyExecuted | Operation already executed | Do not retry |
| 3 | OperationNotFound | Queued operation missing | Verify operation ID |
| 4 | Unauthorized | Caller unauthorized | Use authorized account |

---

# Router Quote Errors

Quote contracts provide route and pricing estimates.

| Code | Error | Meaning | Recovery |
|---------|---------|---------|---------|
| 1 | InvalidQuoteRequest | Request invalid | Correct parameters |
| 2 | NoRouteFound | No valid route available | Try alternative assets |
| 3 | QuoteUnavailable | Quote service unavailable | Retry later |

---

# Error Propagation

## Direct Contract Calls

When a contract invokes another contract:

1. Callee returns an error.
2. Error propagates to caller.
3. Caller may:
   - Bubble error unchanged.
   - Map error into local error type.
   - Handle error and continue.

### Example

```text
RouterCore
  └─ RouterExecution
        └─ DependencyFailure

Result:
DependencyFailure propagates back to RouterCore.
```

---

## Cross-Contract Best Practices

Consumers should:

- Handle all documented error variants.
- Treat unknown errors as failures.
- Log error codes for diagnostics.
- Avoid string-based error matching.
- Differentiate retryable and terminal failures.

---

# Multicall Error Propagation

Multicall execution may contain both required and optional calls.

## Required Calls

If a required call fails:

```text
RequiredCallFailed
```

The entire batch is considered failed.

### Recommended Handling

- Inspect underlying error.
- Correct the failure.
- Submit a new batch.

---

## Optional Calls

Optional calls may fail without causing overall batch failure.

Callers should:

- Review individual call results.
- Handle failed operations separately.
- Retry only retryable failures.

---

# Integration Recommendations

1. Always check returned error codes.
2. Do not rely on error message text.
3. Distinguish retryable from terminal failures.
4. Log propagated multicall failures.
5. Surface actionable recovery guidance to users.
6. Maintain compatibility with future error variants.

---

# Versioning

As new contracts and features are added, additional error variants may be introduced. Integrators should ensure unknown error codes are handled gracefully to maintain forward compatibility.