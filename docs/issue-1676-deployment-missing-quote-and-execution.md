# Issue 1676 — Deployment Guide Missing router-quote and router-execution

**Tracks:** [#1676](https://github.com/Maki-Zeninn/stellar-router/issues/1676)  
**Affects:** `docs/deployment.md`

---

## Problem

`docs/deployment.md` documents deploying and initializing six contracts:
`router-registry`, `router-access`, `router-middleware`, `router-timelock`,
`router-multicall`, and `router-core`. Two contracts that are built, size-tracked
in `.wasm-size-baseline.txt`, released as artifacts, and read by the metrics
exporter are not covered at all:

- `router-quote` (read via `ROUTER_QUOTE_CONTRACT_ID`)
- `router-execution` (read via `ROUTER_EXECUTION_CONTRACT_ID`)

As a result, the mainnet checklist item *"initialize() called on every contract"*
cannot be followed for these two contracts. Operators who follow the guide
deploy an incomplete suite.

---

## Required Changes to `docs/deployment.md`

### Build artifacts

Add the missing artifacts to the build output list:

```
target/wasm32-unknown-unknown/release/router_quote.wasm
target/wasm32-unknown-unknown/release/router_execution.wasm
```

### Deployment order

Insert the two contracts before `router-core`. Neither has hard dependencies,
so they can be deployed in any order after the first five. A reasonable order:

```
1. router-registry   (no dependencies)
2. router-access     (no dependencies)
3. router-middleware (no dependencies)
4. router-timelock   (no dependencies)
5. router-multicall  (no dependencies)
6. router-quote      (no dependencies)
7. router-execution  (no dependencies)
8. router-core       (logically depends on the others, deploy last)
```

### Step 6 — Deploy router-quote

```bash
QUOTE_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/router_quote.wasm \
  --network <NETWORK> --source <ACCOUNT>)
echo "quote: $QUOTE_ID"

stellar contract invoke --id $QUOTE_ID --network <NETWORK> --source <ACCOUNT> \
  -- initialize --admin <ADMIN_ADDRESS> --default_fee_bps 30
```

`--default_fee_bps 30` sets a 0.30% protocol fee. Adjust this to match your
fee schedule before deploying to mainnet.

### Step 7 — Deploy router-execution

```bash
EXECUTION_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/router_execution.wasm \
  --network <NETWORK> --source <ACCOUNT>)
echo "execution: $EXECUTION_ID"

stellar contract invoke --id $EXECUTION_ID --network <NETWORK> --source <ACCOUNT> \
  -- initialize \
  --admin <ADMIN_ADDRESS> \
  --max_retries 3 \
  --backoff_base_ms 0 \
  --backoff_multiplier 100
```

**Parameters:**

| Parameter | Example | Description |
|---|---|---|
| `--max_retries` | `3` | Maximum retry attempts before an execution is marked failed |
| `--backoff_base_ms` | `0` | Base backoff delay in milliseconds (0 = no initial delay) |
| `--backoff_multiplier` | `100` | Multiplier applied to base delay on each retry (ms) |

For mainnet, consider `--backoff_base_ms 500 --backoff_multiplier 200` to avoid
hammering a transiently failing target on retries.

### Environment variables section

Add both IDs to the environment variables table:

| Variable | Default | Description |
|---|---|---|
| `ROUTER_QUOTE_CONTRACT_ID` | — | Deployed router-quote contract ID |
| `ROUTER_EXECUTION_CONTRACT_ID` | — | Deployed router-execution contract ID |

### Mainnet checklist

The existing checklist item:

> - [ ] `initialize()` called on every contract before registering routes

now covers all eight contracts. Explicitly listing them removes ambiguity:

- [ ] `initialize()` called: router-registry, router-access, router-middleware, router-timelock, router-multicall, router-quote, router-execution, router-core

---

## Acceptance Criteria

- [ ] All eight deployable contracts are covered by the deployment guide
- [ ] `router-quote` deploy step includes `initialize --admin <ADMIN> --default_fee_bps 30`
- [ ] `router-execution` deploy step includes `initialize --admin <ADMIN> --max_retries 3 --backoff_base_ms 0 --backoff_multiplier 100`
- [ ] Both contract IDs appear in the environment variables reference table
- [ ] The mainnet checklist explicitly names all eight contracts
