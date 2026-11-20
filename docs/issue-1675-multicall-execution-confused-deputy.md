# Issue 1675 — Confused-Deputy Risk in router-multicall and router-execution

**Tracks:** [#1675](https://github.com/Maki-Zeninn/stellar-router/issues/1675)  
**Affects:** `docs/security.md`, `docs/cookbook.md`, module-level docs for
`router-multicall` and `router-execution`

---

## Problem

`router-multicall::execute_batch` and `router-execution::execute` / `simulate`
accept a caller-chosen `target`, `function`, and `args`. They verify that the
_caller_ is authenticated (`caller.require_auth()`), then invoke the target
contract as themselves:

```
caller → router-multicall (or router-execution) → target_contract
```

From the target contract's perspective, the invoker is the multicall or
execution contract — not the original caller. This matters because:

**Any contract that grants a role, approval, or allowance to the multicall or
execution contract address gives that privilege to every account that can call
`execute_batch` or `execute`.**

Since `require_auth()` passes for any valid signer, there is no additional
restriction. This is a classic confused-deputy vulnerability: the multicall and
execution contracts act as deputies on behalf of their callers, but have no
knowledge of what privileges they carry.

### Concrete example

The `docs/cookbook.md` section *"5. Quote Comparison & Batched Execution"*
shows users batching calls to DEX swap functions through multicall:

```json
{"target": "<DEX_A>", "function": "swap", "required": true, ...}
```

If a DEX or token contract trusts the multicall address (e.g., as a result of
calling `approve` with the multicall address as the spender), any user can
drain that approval by routing a `transfer_from` call through multicall.

---

## Security Rule

> **Never grant roles, admin rights, or token allowances to `router-multicall`
> or `router-execution` contract addresses.**

The authorization check in these contracts confirms the _caller's_ identity,
not the _target's_ trust relationship with them. Privileges granted to the
contract address are usable by any account that can invoke them.

---

## Required Changes to `docs/security.md`

Add the following as **Threat 8** in the Threats and Mitigations section:

---

### 8. Confused-Deputy Attack via router-multicall and router-execution

**What can an attacker do?**

If any contract has granted `router-multicall` or `router-execution` a role,
admin right, or token allowance, any authenticated caller can exploit that
privilege by routing a call through the respective contract. The target sees
the multicall/execution address as its invoker, not the original caller, and
acts on the trust it has extended.

**Why this happens**

`execute_batch` and `execute` check `caller.require_auth()` — confirming the
calling account signed the transaction — but then issue `env.invoke_contract`
with the target chosen by the caller. The multicall and execution contracts
carry no context about what privileges they hold; they simply pass the call
through.

**Mitigations**

- **Never grant roles in router-access to these contract addresses.**
  Any role held by the multicall or execution address is effectively held by
  every user who can call `execute_batch` or `execute`.
- **Never set these contracts as admin of any other contract.**
- **Never approve token allowances to these contract addresses.**
  DEX integrations and cookbook examples must call `approve` with the actual
  user's address as the spender, not the multicall address.
- **Consider an admin-managed allow-list** (target, function) pairs that
  multicall and execution are permitted to invoke. This restricts the attack
  surface to explicitly approved operations and makes the contracts less
  general-purpose but substantially safer for privileged deployments.

**Decision to record**

The team should decide whether to:
1. Keep the contracts fully open (current behaviour) with strong documentation
   warnings, or
2. Add an admin-managed allow-list of permitted `(target, function)` pairs.

The choice and rationale must be recorded in this section.

**Deployment checklist addition**

- [ ] No role in router-access is held by the multicall or execution contract address
- [ ] No token allowances exist for the multicall or execution contract address
- [ ] `docs/cookbook.md` DEX examples use the user's address (not the multicall address) as the token spender

---

## Required Changes to `docs/cookbook.md`

In **Section 5 — Quote Comparison & Batched Execution**, add a warning before
the "Batch execution via multicall" subsection:

> **⚠ Token approvals and the multicall address**
>
> Do **not** call `approve` with the multicall contract address as the spender
> before batching a swap. If you do, any account with access to `execute_batch`
> can transfer those tokens. Instead, structure your swap calls so the token
> contract sees the user's own address as the spender, or use an
> atomic-approve-and-swap pattern where approval and use happen in the same
> transaction.
>
> See [security.md — Confused-Deputy Attack](./security.md#8-confused-deputy-attack-via-router-multicall-and-router-execution)
> for the full explanation.

---

## Required Changes to Contract Module Docs

Both contracts should carry a module-level doc comment explaining the
invoker-identity behaviour:

### router-multicall (contracts/router-multicall/src/lib.rs)

```rust
//! # Security — Invoker Identity
//!
//! `execute_batch` checks the caller's authentication (`caller.require_auth()`)
//! but invokes each target as *this contract*. Target contracts see the
//! multicall contract address as their invoker, not the original caller.
//!
//! **Never grant roles, admin rights, or token allowances to this contract's
//! address.** Any such privilege is usable by any account that can call
//! `execute_batch`. See `docs/security.md` for the full threat model.
```

### router-execution (contracts/router-execution/src/lib.rs)

```rust
//! # Security — Invoker Identity
//!
//! `execute` and `simulate` invoke target contracts as *this contract*.
//! Target contracts see the execution contract address as their invoker,
//! not the account that submitted the transaction.
//!
//! **Never grant roles, admin rights, or token allowances to this contract's
//! address.** Any such privilege is usable by any account that can call
//! `execute`. See `docs/security.md` for the full threat model.
```

---

## Acceptance Criteria

- [ ] `docs/security.md` describes the invoker-identity behaviour and the
      "never grant privileges to these contracts" rule as a named threat
- [ ] Decision on whether to add an allow-list is recorded in `docs/security.md`
- [ ] `docs/cookbook.md` DEX batching examples include the token-approval warning
- [ ] Module-level doc comments in `router-multicall` and `router-execution`
      describe the invoker-identity behaviour
- [ ] Deployment checklist in `docs/deployment.md` includes the three items
      above
