# syntax=docker/dockerfile:1
# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.88-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends curl && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY contracts/ contracts/

RUN cargo build \
    --package router-common \
    --package router-core \
    --package router-registry \
    --package router-access \
    --package router-middleware \
    --package router-timelock \
    --package router-multicall \
    --package router-quote \
    --package router-execution

# ── Test stage ────────────────────────────────────────────────────────────────
FROM builder AS test
CMD ["cargo", "test", \
    "--package", "router-common", \
    "--package", "router-core", \
    "--package", "router-registry", \
    "--package", "router-access", \
    "--package", "router-middleware", \
    "--package", "router-timelock", \
    "--package", "router-multicall", \
    "--package", "router-quote", \
    "--package", "router-execution"]

# ── WASM build stage ──────────────────────────────────────────────────────────
FROM builder AS wasm
RUN cargo build --target wasm32-unknown-unknown --release \
    --package router-core \
    --package router-registry \
    --package router-access \
    --package router-middleware \
    --package router-timelock \
    --package router-multicall \
    --package router-execution

# ── Metrics exporter runtime ──────────────────────────────────────────────────
FROM debian:bookworm-slim AS metrics
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
EXPOSE 9090
