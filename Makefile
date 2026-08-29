.PHONY: fmt-check check test-all lint build-wasm

fmt-check:
	cargo fmt --check
	cargo fmt --manifest-path metrics/Cargo.toml --check
	cargo fmt --manifest-path api-server/Cargo.toml --check

check:
	cargo check --workspace

test-all:
	cargo test --workspace
	cargo test --manifest-path metrics/Cargo.toml
	cargo test --manifest-path api-server/Cargo.toml

lint:
	cargo clippy --workspace -- -D warnings
	cargo clippy --manifest-path metrics/Cargo.toml -- -D warnings
	cargo clippy --manifest-path api-server/Cargo.toml -- -D warnings

build-wasm:
	cargo build --target wasm32-unknown-unknown --release
