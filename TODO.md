# TODO

- [ ] Update `contracts/router-access/src/lib.rs` storage: add `DataKey::RoleMemberCount(String)`.
- [ ] Add view function `get_role_member_count(role: String) -> u32`.
- [ ] Maintain counter in `grant_role_internal`.
- [ ] Maintain counter in `revoke_role`.
- [ ] Maintain counter in `expire_role`.
- [ ] Add/extend unit tests for member count behavior.
- [ ] Run `cargo test` to verify compilation + passing tests.

