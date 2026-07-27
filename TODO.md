# TODO

## Role membership transfer between addresses (router-access)
- [ ] Add new contract API to transfer role membership from `from` to `to`.
- [ ] Enforce authorization: caller must be role manager for the role (or super admin if designed that way).
- [ ] Semantics: transfer only if `from` currently has the role **active** (expired source must error).
- [ ] Preserve expiry: destination receives the same expiry timestamp as source.
- [ ] Keep storage consistent:
  - [ ] Update `HasRole`, `RoleExpiry`
  - [ ] Update `RoleMembers(role)`
  - [ ] Update `AddressRoles(from)` and `AddressRoles(to)`
  - [ ] Update `RoleMemberCount(role)` correctly only when active.
- [ ] Add events (either new event topic or emit existing grant/revoke events consistently).
- [ ] Add tests:
  - [ ] permanent grant transfer
  - [ ] expiring grant transfer (expiry preserved)
  - [ ] expired source transfer fails
  - [ ] destination blacklisting prevents transfer
  - [ ] role not present on source fails
  - [ ] `from == to` behavior
- [ ] Run `cargo test` for the affected contract/package(s).

