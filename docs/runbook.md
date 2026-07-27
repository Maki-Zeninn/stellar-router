# Operational Runbook

Procedures for common production scenarios when operating the stellar-router contract suite.

## Prerequisites

All commands assume you have the Stellar CLI installed and configured:

```bash
stellar contract invoke --id <CONTRACT_ID> --network <NETWORK> --source <ADMIN_KEY> -- <FUNCTION> [ARGS]
```

Replace placeholders:
- `<CORE_ID>` — router-core contract ID
- `<REGISTRY_ID>` — router-registry contract ID
- `<ACCESS_ID>` — router-access contract ID
- `<MIDDLEWARE_ID>` — router-middleware contract ID
- `<TIMELOCK_ID>` — router-timelock contract ID
- `<ADMIN>` — current admin address
- `<NETWORK>` — `testnet` or `mainnet`

---

## 1. Emergency: Pause All Routing

Use when you need to immediately stop all route resolution (e.g. during a security incident).

### Pause

```bash
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- set_paused --caller <ADMIN> --paused true
```

### Verify the pause is active

Attempt to resolve any known route. It should return `RouterPaused` (error code 6):

```bash
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- resolve --name <ANY_ROUTE_NAME>
# Expected: Error RouterPaused (6)
```

### Resume

Once the incident is resolved:

```bash
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- set_paused --caller <ADMIN> --paused false
```

Verify by resolving a known route successfully:

```bash
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- resolve --name <KNOWN_ROUTE>
# Expected: returns the contract address
```

### Pause a single route

If only one route is compromised:

```bash
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- set_route_paused --caller <ADMIN> --name <ROUTE_NAME> --paused true
```

Verify:

```bash
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- resolve --name <ROUTE_NAME>
# Expected: Error RoutePaused (5)
```

---

## 2. Incident: Circuit Breaker / Rate Limit Triggered

The middleware contract enforces per-caller rate limits. When a caller exceeds the limit, they receive `RateLimitExceeded` (error code 4).

### Check rate limit state for a caller

```bash
stellar contract invoke --id <MIDDLEWARE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- rate_limit_state --caller <CALLER_ADDRESS>
```

This returns `calls_in_window` and `window_start`. Compare against the route config.

### Check route configuration

```bash
stellar contract invoke --id <MIDDLEWARE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- route_config --route <ROUTE_NAME>
```

Returns `max_calls_per_window`, `window_seconds`, and `enabled`.

### Temporarily increase rate limit

If the rate limit is too aggressive:

```bash
stellar contract invoke --id <MIDDLEWARE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- configure_route \
  --caller <ADMIN> \
  --route <ROUTE_NAME> \
  --max_calls_per_window <NEW_LIMIT> \
  --window_seconds <WINDOW> \
  --enabled true
```

### Disable middleware globally (emergency only)

```bash
stellar contract invoke --id <MIDDLEWARE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- set_global_enabled --caller <ADMIN> --enabled false
```

### Investigate root cause

1. Check `total_calls` to see overall call volume:
   ```bash
   stellar contract invoke --id <MIDDLEWARE_ID> --network <NETWORK> --source <ADMIN_KEY> \
     -- total_calls
   ```
2. Review contract events for `pre_call` and `post_call` entries to identify the caller and route.
3. Check if the route is disabled vs. rate-limited (error code 5 = `RouteDisabled`, error code 4 = `RateLimitExceeded`).

---

## 3. Routine: Rotate Admin Keys

Each contract has its own admin. Rotate all of them when changing operators.

### Step-by-step admin transfer

For non-emergency transfers, use the timelock to queue the change:

```bash
# Queue admin transfer with 24h delay
stellar contract invoke --id <TIMELOCK_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- queue \
  --proposer <ADMIN> \
  --description "rotate admin to <NEW_ADMIN>" \
  --target <CORE_ID> \
  --delay 86400
```

For emergency transfers, transfer directly on each contract:

**router-core:**
```bash
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- transfer_admin --current <ADMIN> --new_admin <NEW_ADMIN>
```

**router-registry:**
```bash
stellar contract invoke --id <REGISTRY_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- transfer_admin --current <ADMIN> --new_admin <NEW_ADMIN>
```

**router-access:**
```bash
stellar contract invoke --id <ACCESS_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- transfer_super_admin --current <ADMIN> --new_admin <NEW_ADMIN>
```

**router-middleware** and **router-timelock** do not expose `transfer_admin`. Redeployment is required to change their admin.

### Verification checklist

After transferring, verify the new admin is set on each contract:

```bash
# Verify router-core admin
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <NEW_ADMIN_KEY> \
  -- admin
# Expected: <NEW_ADMIN>

# Verify router-registry admin
stellar contract invoke --id <REGISTRY_ID> --network <NETWORK> --source <NEW_ADMIN_KEY> \
  -- admin
# Expected: <NEW_ADMIN>

# Verify router-access super admin
stellar contract invoke --id <ACCESS_ID> --network <NETWORK> --source <NEW_ADMIN_KEY> \
  -- super_admin
# Expected: <NEW_ADMIN>
```

Confirm the old admin can no longer perform admin operations:

```bash
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <OLD_ADMIN_KEY> \
  -- set_paused --caller <OLD_ADMIN> --paused true
# Expected: Error Unauthorized (3)
```

---

## 4. Routine: Register a New Route

Full checklist for adding a new route to the system.

### Step 1: Register in the registry

```bash
stellar contract invoke --id <REGISTRY_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- register \
  --caller <ADMIN> \
  --name <ROUTE_NAME> \
  --address <CONTRACT_ADDRESS> \
  --version 1
```

### Step 2: Add to router-core

```bash
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- register_route \
  --caller <ADMIN> \
  --name <ROUTE_NAME> \
  --address <CONTRACT_ADDRESS>
```

### Step 3: Configure middleware

```bash
stellar contract invoke --id <MIDDLEWARE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- configure_route \
  --caller <ADMIN> \
  --route <ROUTE_NAME> \
  --max_calls_per_window 100 \
  --window_seconds 3600 \
  --enabled true
```

### Step 4: Set up access control (if needed)

```bash
# Grant operator role to the route's service account
stellar contract invoke --id <ACCESS_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- grant_role \
  --caller <ADMIN> \
  --role operator \
  --target <SERVICE_ACCOUNT>
```

### Validation after registration

```bash
# 1. Verify route resolves correctly
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- resolve --name <ROUTE_NAME>
# Expected: returns <CONTRACT_ADDRESS>

# 2. Verify route entry details
stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- get_route --name <ROUTE_NAME>
# Expected: RouteEntry with paused=false

# 3. Verify registry entry
stellar contract invoke --id <REGISTRY_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- get --name <ROUTE_NAME> --version 1
# Expected: ContractEntry with deprecated=false

# 4. Verify middleware config
stellar contract invoke --id <MIDDLEWARE_ID> --network <NETWORK> --source <ADMIN_KEY> \
  -- route_config --route <ROUTE_NAME>
# Expected: RouteConfig with enabled=true
```

---

## 5. Debugging: Route Resolution Failures

### Error codes

| Contract | Code | Error | Cause |
|---|---|---|---|
| router-core | 1 | `AlreadyInitialized` | `initialize` called twice |
| router-core | 2 | `NotInitialized` | Contract not initialized yet |
| router-core | 3 | `Unauthorized` | Caller is not the admin |
| router-core | 4 | `RouteNotFound` | Route name not registered |
| router-core | 5 | `RoutePaused` | Individual route is paused |
| router-core | 6 | `RouterPaused` | Entire router is paused |
| router-core | 7 | `RouteAlreadyExists` | Route name already registered |
| router-middleware | 4 | `RateLimitExceeded` | Caller exceeded rate limit |
| router-middleware | 5 | `RouteDisabled` | Route disabled in middleware |
| router-middleware | 6 | `MiddlewareDisabled` | All middleware disabled globally |
| router-registry | 4 | `NotFound` | Entry not in registry |
| router-registry | 6 | `AlreadyDeprecated` | Version already deprecated |
| router-access | 6 | `Blacklisted` | Address is blacklisted |

### Diagnostic steps

1. **Check if the router is paused:**
   ```bash
   stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
     -- resolve --name <ROUTE_NAME>
   ```
   - Error 6 (`RouterPaused`): the entire router is paused.
   - Error 5 (`RoutePaused`): this specific route is paused.
   - Error 4 (`RouteNotFound`): the route was never registered or was removed.

2. **Check route details:**
   ```bash
   stellar contract invoke --id <CORE_ID> --network <NETWORK> --source <ADMIN_KEY> \
     -- get_route --name <ROUTE_NAME>
   ```

3. **Check middleware state:**
   ```bash
   stellar contract invoke --id <MIDDLEWARE_ID> --network <NETWORK> --source <ADMIN_KEY> \
     -- route_config --route <ROUTE_NAME>
   ```

4. **Check access control:**
   ```bash
   stellar contract invoke --id <ACCESS_ID> --network <NETWORK> --source <ADMIN_KEY> \
     -- has_role --role operator --target <CALLER_ADDRESS>

   stellar contract invoke --id <ACCESS_ID> --network <NETWORK> --source <ADMIN_KEY> \
     -- is_blacklisted --target <CALLER_ADDRESS>
   ```

### Metrics to monitor

- `total_routed` on router-core: total successful resolutions
- `total_calls` on router-middleware: total pre-call hook invocations
- `total_batches` on router-multicall: total batch executions
- Contract events: `route_registered`, `route_updated`, `route_removed`, `routed`, `pre_call`, `post_call`, `call_result`
