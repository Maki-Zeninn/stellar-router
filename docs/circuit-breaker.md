# Circuit Breaker

The circuit breaker in `router-middleware` protects downstream contracts from
cascading failures by tracking per-route failure counts and temporarily blocking
calls when a failure threshold is reached.

## State Machine

```
                     failure_count >= failure_threshold
   ┌─────────────────────────────────────────────────────────────────┐
   │                                                                 ▼
┌──┴──────┐    failure_count >= failure_threshold              ┌──────────┐
│ Closed  ├───────────────────────────────────────────────────►│   Open   │
│         │                                                     │          │
│ Calls   │◄────────────────────────────────────────────────── │ Calls    │
│ allowed │         success (probe)              ┌─────────────┤ blocked  │
└────┬────┘                                      │             └──────────┘
     │                                           │                  │
     │ success resets                       ┌────▼──────┐          │ recovery_window_seconds
     │ failure_count to 0                   │ Half-Open │          │ elapsed since opened_at
     │                                      │           │◄─────────┘
     │                                      │ One probe │
     │                                      │ call let  │
     │                                      │ through   │
     │                                      └─────┬─────┘
     │                                            │
     │                                            │ failure (probe)
     │                                            │ resets opened_at,
     │                                            │ failure_count = 1
     │                                            ▼
     │                                       (back to Open)
     └───────────────────────────────────────────────────────────────
```

### Transition Summary

| From      | To        | Trigger                                                          |
|-----------|-----------|------------------------------------------------------------------|
| Closed    | Open      | `failure_count >= failure_threshold` after a failed call        |
| Open      | Half-Open | `recovery_window_seconds` elapsed since `opened_at`             |
| Half-Open | Closed    | A successful probe call                                          |
| Half-Open | Open      | A failed probe call — resets `opened_at`, sets `failure_count = 1` |
| Closed    | Closed    | A successful call resets `failure_count` to 0                   |

## Configuration Parameters

Both parameters are set per-route via `configure_route` in `router-middleware`.

| Parameter                  | Type  | Description                                                                                                |
|----------------------------|-------|------------------------------------------------------------------------------------------------------------|
| `failure_threshold`        | `u32` | Number of consecutive failures required to open the circuit. Set to `0` to disable the circuit breaker.   |
| `recovery_window_seconds`  | `u64` | Seconds that must elapse after `opened_at` before the circuit transitions to Half-Open for a probe call.  |

Timing uses `env.ledger().timestamp()` (UNIX seconds from the Soroban ledger).

## State Fields (`CircuitBreakerState`)

| Field           | Type   | Description                                              |
|-----------------|--------|----------------------------------------------------------|
| `failure_count` | `u32`  | Failures accumulated in the current Closed window        |
| `opened_at`     | `u64`  | Ledger timestamp when the circuit was last opened        |
| `is_open`       | `bool` | True when the circuit is Open (calls blocked)            |
| `is_half_open`  | `bool` | True when the circuit is Half-Open (one probe allowed)   |

## Code Location

- State machine logic: `contracts/router-middleware/src/circuit_breaker.rs`
- Integration point: `pre_call` / `post_call` in `contracts/router-middleware/src/lib.rs`
- Config: `RouteConfig` struct in the same file

## Recommended Settings

### Low-latency, high-availability service (e.g. oracle)
```
failure_threshold        = 3
recovery_window_seconds  = 30
```
Opens quickly on repeated failures; probes frequently.

### Batch processing / infrequent calls
```
failure_threshold        = 5
recovery_window_seconds  = 300
```
Tolerates transient blips; waits 5 minutes before probing.

### Disable circuit breaker entirely
```
failure_threshold        = 0
recovery_window_seconds  = 0
```
Setting `failure_threshold` to `0` bypasses all circuit-breaker logic.

## Example: Configure via CLI

```bash
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source admin \
  -- configure_route \
  --caller <ADMIN_ADDRESS> \
  --route "oracle/get_price" \
  --max_calls_per_window 100 \
  --window_seconds 3600 \
  --enabled true \
  --failure_threshold 3 \
  --recovery_window_seconds 30 \
  --log_retention 0
```

## Events

| Event name       | Emitted when                                 | Payload                          |
|------------------|----------------------------------------------|----------------------------------|
| `circuit_opened` | Circuit transitions to Open (from Closed or Half-Open) | `(route_name, failure_count)` |

No event is emitted on Close or Half-Open transitions; monitor `post_call` logs
for those signals.

## Manual Reset

An admin can reset the circuit breaker for a route back to Closed state without
waiting for the recovery window:

```bash
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source admin \
  -- reset_circuit_breaker \
  --caller <ADMIN_ADDRESS> \
  --route "oracle/get_price"
```
