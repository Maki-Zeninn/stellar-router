# Contract Interaction Cookbook

Practical end-to-end examples showing how to use multiple stellar-router contracts together in common on-chain scenarios.

All examples assume contracts are deployed and initialized on Stellar testnet. Replace `<CORE_ID>`, `<REGISTRY_ID>`, etc. with actual deployed contract IDs.

---

## 1. Route Registration & Discovery

Register a DEX route in the core router and a versioned entry in the registry, then discover and resolve it.

### Register on core

```bash
# Initialize core (one-time)
stellar contract invoke --id <CORE_ID> --network testnet --source admin \
  -- initialize --admin <ADMIN>

# Register a DEX route
stellar contract invoke --id <CORE_ID> --network testnet --source admin \
  -- register_route \
  --caller <ADMIN> \
  --name amm_swap \
  --address <DEX_CONTRACT_ID> \
  --metadata '{"description": "Primary AMM swap route", "tags": ["dex", "amm"], "owner": "<ADMIN>"}'

# Create a friendly alias
stellar contract invoke --id <CORE_ID> --network testnet --source admin \
  -- add_alias \
  --caller <ADMIN> \
  --existing_name amm_swap \
  --alias_name swap
```

### Register versioned entry on registry

```bash
# Initialize registry (one-time)
stellar contract invoke --id <REGISTRY_ID> --network testnet --source admin \
  -- initialize --admin <ADMIN>

# Register v1 of the DEX
stellar contract invoke --id <REGISTRY_ID> --network testnet --source admin \
  -- register \
  --caller <ADMIN> \
  --name amm_swap \
  --address <DEX_CONTRACT_ID> \
  --version 1

# Deploy a new version and register it
stellar contract invoke --id <REGISTRY_ID> --network testnet --source admin \
  -- register \
  --caller <ADMIN> \
  --name amm_swap \
  --address <DEX_V2_CONTRACT_ID> \
  --version 2
```

### Discover and resolve

```bash
# Resolve via core
stellar contract invoke --id <CORE_ID> --network testnet --source user \
  -- resolve --name amm_swap

# Resolve via alias
stellar contract invoke --id <CORE_ID> --network testnet --source user \
  -- resolve --name swap

# Get latest from registry
stellar contract invoke --id <REGISTRY_ID> --network testnet --source user \
  -- get_latest --name amm_swap

# List all registered routes
stellar contract invoke --id <CORE_ID> --network testnet --source user \
  -- get_all_routes
```

---

## 2. Access Control Setup

Create a role hierarchy (super-admin `→` protocol-admin `→` route-manager), grant roles with expiry, and blacklist a compromised address.

### Initialize and set up roles

```bash
# Initialize access contract
stellar contract invoke --id <ACCESS_ID> --network testnet --source admin \
  -- initialize --super_admin <ADMIN>

# Create a protocol-admin role and assign it
stellar contract invoke --id <ACCESS_ID> --network testnet --source admin \
  -- grant_role \
  --caller <ADMIN> \
  --role protocol-admin \
  --target <PROTOCOL_ADMIN> \
  --expires_at '{"void": null}'

# Make PROTOCOL_ADMIN the role admin for route-manager
stellar contract invoke --id <ACCESS_ID> --network testnet --source admin \
  -- set_role_admin \
  --caller <ADMIN> \
  --role route-manager \
  --admin <PROTOCOL_ADMIN>

# Protocol-admin grants route-manager to an operator (expires at ledger 4000000)
stellar contract invoke --id <ACCESS_ID> --network testnet --source admin \
  -- grant_role \
  --caller <PROTOCOL_ADMIN> \
  --role route-manager \
  --target <OPERATOR> \
  --expires_at '{"Some": 4000000}'
```

### Check access before operations

```bash
# Verify operator still holds the role
stellar contract invoke --id <ACCESS_ID> --network testnet --source user \
  -- has_role --role route-manager --target <OPERATOR>
```

### Blacklist a compromised address

```bash
# Blacklist a leaked key
stellar contract invoke --id <ACCESS_ID> --network testnet --source admin \
  -- blacklist --caller <ADMIN> --target <COMPROMISED_ADDR>

# Check returns false even if role was granted
stellar contract invoke --id <ACCESS_ID> --network testnet --source user \
  -- has_role --role route-manager --target <COMPROMISED_ADDR>

# Unblacklist after recovery
stellar contract invoke --id <ACCESS_ID> --network testnet --source admin \
  -- unblacklist --caller <ADMIN> --target <COMPROMISED_ADDR>
```

---

## 3. Rate-Limited Route with Circuit Breaker

Configure middleware for a high-traffic route with rate limiting and automatic circuit breaker that opens after repeated failures.

### Configure middleware

```bash
# Initialize middleware
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source admin \
  -- initialize --admin <ADMIN>

# Configure route: 100 calls/hour, circuit opens after 5 failures, 300s recovery
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source admin \
  -- configure_route \
  --caller <ADMIN> \
  --route amm_swap \
  --max_calls_per_window 100 \
  --window_seconds 3600 \
  --enabled true \
  --failure_threshold 5 \
  --recovery_window_seconds 300
```

### Pre-call check and post-call reporting

```bash
# Before routing, check rate limit and circuit state
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source user \
  -- pre_call --caller <USER> --route amm_swap

# Execute the actual swap
stellar contract invoke --id <DEX_CONTRACT_ID> --network testnet --source user \
  -- swap --token_in <TOKEN_A> --token_out <TOKEN_B> --amount 1000

# Report back to middleware
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source user \
  -- post_call --caller <USER> --route amm_swap --success true
```

### Inspect and reset circuit breaker

```bash
# Check rate limit state
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source user \
  -- rate_limit_state --route amm_swap --caller <USER>

# Check route config
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source user \
  -- route_config --route amm_swap

# Admin resets breaker after fixing downstream issues
stellar contract invoke --id <MIDDLEWARE_ID> --network testnet --source admin \
  -- reset_circuit_breaker --caller <ADMIN> --route amm_swap
```

---

## 4. Timelocked Admin Operations

Queue an admin transfer with a 24-hour delay, execute after the ETA, and emergency cancel.

### Queue an operation

```bash
# Initialize timelock with 24h minimum delay
stellar contract invoke --id <TIMELOCK_ID> --network testnet --source admin \
  -- initialize --admin <ADMIN> --min_delay 86400

# Queue an admin transfer with 24h delay
stellar contract invoke --id <TIMELOCK_ID> --network testnet --source admin \
  -- queue \
  --proposer <ADMIN> \
  --description "Transfer router admin to multisig" \
  --target <CORE_ID> \
  --delay 86400 \
  --depends_on '["void"]'
```

The command outputs an `op_id`. Save it for execution.

### Execute after delay

```bash
# After 24h, check the operation
stellar contract invoke --id <TIMELOCK_ID> --network testnet --source admin \
  -- get_op --op_id 1

# Execute the queued operation
stellar contract invoke --id <TIMELOCK_ID> --network testnet --source admin \
  -- execute --caller <ADMIN> --op_id 1

# Verify the transfer
stellar contract invoke --id <CORE_ID> --network testnet --source user \
  -- admin
```

### Emergency cancel

```bash
# Cancel before ETA
stellar contract invoke --id <TIMELOCK_ID> --network testnet --source admin \
  -- cancel --caller <ADMIN> --op_id 1

# Bulk cancel all pending operations
stellar contract invoke --id <TIMELOCK_ID> --network testnet --source admin \
  -- cancel_all --admin <ADMIN>
```

---

## 5. Multi-Hop Quote & Execution

Get quotes for multiple routes, pick the best one, and execute a batch of swaps via multicall.

### Configure quote fees

```bash
# Initialize quote contract with 0.5% default fee
stellar contract invoke --id <QUOTE_ID> --network testnet --source admin \
  -- initialize --admin <ADMIN> --default_fee_bps 50

# Set per-route fees
stellar contract invoke --id <QUOTE_ID> --network testnet --source admin \
  -- set_route_fee --caller <ADMIN> --route uniswap --fee_bps 30

stellar contract invoke --id <QUOTE_ID> --network testnet --source admin \
  -- set_route_fee --caller <ADMIN> --route sushiswap --fee_bps 10
```

### Compare quotes

```bash
# Get individual quote
stellar contract invoke --id <QUOTE_ID> --network testnet --source user \
  -- get_quote \
  --request '{"route": "uniswap", "token_in": "<TOKEN_A>", "token_out": "<TOKEN_B>", "amount_in": 1000000}'

# Get quotes for multiple routes
stellar contract invoke --id <QUOTE_ID> --network testnet --source user \
  -- get_quotes \
  --requests '[{"route": "uniswap", "token_in": "<TOKEN_A>", "token_out": "<TOKEN_B>", "amount_in": 1000000}, {"route": "sushiswap", "token_in": "<TOKEN_A>", "token_out": "<TOKEN_B>", "amount_in": 1000000}]'

# Auto-pick the best quote
stellar contract invoke --id <QUOTE_ID> --network testnet --source user \
  -- get_best_quote \
  --requests '[{"route": "uniswap", "token_in": "<TOKEN_A>", "token_out": "<TOKEN_B>", "amount_in": 1000000}, {"route": "sushiswap", "token_in": "<TOKEN_A>", "token_out": "<TOKEN_B>", "amount_in": 1000000}]'
```

### Batch execution via multicall

```bash
# Initialize multicall
stellar contract invoke --id <MULTICALL_ID> --network testnet --source admin \
  -- initialize --admin <ADMIN> --max_batch_size 10

# Execute two swaps in one batch
stellar contract invoke --id <MULTICALL_ID> --network testnet --source user \
  -- execute_batch \
  --caller <USER> \
  --calls '[{"target": "<DEX_A>", "function": "swap", "required": true, "budget": null}, {"target": "<DEX_B>", "function": "swap", "required": false, "budget": null}]' \
  --simulate false
```

### Check multicall stats

```bash
stellar contract invoke --id <MULTICALL_ID> --network testnet --source user \
  -- total_batches

stellar contract invoke --id <MULTICALL_ID> --network testnet --source user \
  -- max_batch_size
```
