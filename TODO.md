# Task: Add test for resolving non-existent route in router-core

## Steps:
1. [x] Add new unit test `test_resolve_unknown_route_fails` to `contracts/router-core/src/lib.rs`
2. [x] Create corresponding snapshot file `contracts/router-core/test_snapshots/tests/test_resolve_unknown_route_fails.1.json`
3. [x] Run `cd contracts/router-core && cargo test` to verify tests pass and snapshots are correct
4. [ ] Mark complete and attempt_completion

Current progress: All steps complete. Test passes ✅

# TODO: Implement blackboxai/issue-9-get-all-routes

## Steps to complete:
- [x] Step 1: Add `pub fn get_all_routes(env: Env) -> Vec<String>` function with doc comment and storage iteration logic.
- [x] Step 2: Add 2 tests (empty router and multiple routes) in test module.
- [ ] Step 3: Commit changes.

Progress will be updated as steps complete.
