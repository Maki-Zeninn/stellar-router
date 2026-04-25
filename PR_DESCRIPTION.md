## Summary

Remove the stray `benches/src/scrap-menu/` bench-source leftovers and prevent them
from being committed again.

## What Changed

- deleted the `benches/src/scrap-menu/` directory from the repository
- added `benches/src/scrap-menu/` to `.gitignore`

## Why

The files in `benches/src/scrap-menu/` do not belong to this Rust workspace. Their
NestJS-style names are unrelated to the Soroban contracts in this repository, add
noise for contributors, and risk confusing future maintenance.

## Verification

- `cargo build --workspace`
- `cargo test --workspace`
