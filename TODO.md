- [x] Explore router-multicall contract code and existing tests (read contracts/router-multicall/src/lib.rs)
- [x] Add/adjust tests to verify stored batch results behavior via get_batch_result/get_batch_results
- [x] Fix compilation issues in tests due to Soroban client return types (Option/Vec handling)
- [x] Run `cargo test -p router-multicall` and ensure tests pass
# TODO

- [ ] Inspect router-access role/expiry logic and existing tests for `grant_role`.
- [x] Implement behavior change in `contracts/router-access/src/lib.rs`:
  - [x] If role assignment exists and is unexpired:
    - [x] Return `AlreadyHasRole` only if requested expiry timestamp matches existing.
    - [x] Otherwise update `RoleExpiry` to the new expiry timestamp and return `Ok`.
- [x] Add new tests covering:
  - [x] Duplicate grant without expiry: AlreadyHasRole
  - [x] Extend expiry: update works
  - [x] Shorten expiry: update works
  - [x] Grant after expiry: succeeds
  - [x] Grant with None expiry over existing expiry: updates to permanent
- [x] Run `cargo test -p router-access` to confirm.


