.PHONY: fmt-check test-all lint build-wasm

fmt-check:
	cargo fmt --check

test-all:
	cargo test --workspace

lint:
	cargo clippy -- -D warnings

build-wasm:
	cargo build --target wasm32-unknown-unknown --release
