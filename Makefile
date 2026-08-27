.PHONY: fmt-check check test-all lint build-wasm

fmt-check:
	cargo fmt --check

check:
	cargo check --workspace

test-all:
	cargo test --workspace

lint:
	cargo clippy --workspace -- -D warnings

build-wasm:
	cargo build --target wasm32-unknown-unknown --release
