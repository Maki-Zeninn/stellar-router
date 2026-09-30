# Issue #1266 — Remove Stray Placeholder Comment in `router-access` Test Module

**Tracking:** [#1266](https://github.com/Maki-Zeninn/stellar-router/issues/1266)  
**Component:** `contracts/router-access`  
**Type:** Refactor / Code Hygiene  
**Impact:** Cosmetic

---

## Summary

A leftover placeholder comment exists in the `router-access` test module immediately after
the `setup()` helper and before the first `#[test]` function. The comment reads:

```rust
// ... (all your existing tests remain unchanged) ...
```

This is an artifact of an editing or diff instruction — the kind of shorthand used when
describing a partial change ("leave everything below this point as-is"). It was never
meaningful documentation and was accidentally committed as part of the source file.

---

## Location

**File:** `contracts/router-access/src/lib.rs`  
**Approximate line:** 1016

```rust
    fn setup() -> (Env, Address, RouterAccessClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RouterAccess);
        let client = RouterAccessClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, admin, client)
    }

    // ... (all your existing tests remain unchanged) ...   <-- remove this line

    #[test]
    fn test_grant_role_fails_when_max_roles_exceeded() {
```

---

## Why It's Problematic

- A new contributor scanning the file would likely read this as a TODO, a sign that
  content has been elided, or an indication that the test suite is incomplete.
- It creates unnecessary noise in code review diffs and `git blame` output.
- It sets a precedent for keeping non-informational comments in the codebase.

---

## Required Change

Delete the single comment line. No logic changes, no test changes, no refactoring of
surrounding code is needed.

```diff
-    // ... (all your existing tests remain unchanged) ...
```

---

## Acceptance Criteria

- [ ] The comment line `// ... (all your existing tests remain unchanged) ...` is absent
      from `contracts/router-access/src/lib.rs`.
- [ ] No other lines in the file are modified.
- [ ] All existing tests continue to pass without modification.
