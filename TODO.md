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


