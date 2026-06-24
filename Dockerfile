FROM rust:1.79-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY contracts/ contracts/
RUN cargo build --release
RUN cargo build --target wasm32-unknown-unknown --release

# ── Test stage ────────────────────────────────────────────────────────────────

FROM builder AS test

RUN cargo test --release

# ── WASM artifacts stage ──────────────────────────────────────────────────────

FROM debian:bookworm-slim AS wasm

COPY --from=builder /app/target/wasm32-unknown-unknown/release/*.wasm /wasm/

# ── API server stage ─────────────────────────────────────────────────────────

FROM debian:bookworm-slim AS api-server

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/ /usr/local/bin/
COPY --from=builder /app/target/wasm32-unknown-unknown/release/*.wasm /wasm/

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

CMD ["stellar-router-api"]

# ── Metrics exporter stage ───────────────────────────────────────────────────

FROM debian:bookworm-slim AS metrics

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/ /usr/local/bin/

EXPOSE 9090

HEALTHCHECK --interval=30s --timeout=10s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:9090/metrics || exit 1

CMD ["stellar-router-metrics"]
# syntax=docker/dockerfile:1
# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.88-slim AS builder

# Install wasm32 target for contract compilation
RUN apt-get update && apt-get install -y --no-install-recommends curl && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown

WORKDIR /app

# Cache dependencies before copying source
COPY Cargo.toml ./
COPY contracts/ contracts/
COPY metrics/ metrics/
COPY integration-tests/ integration-tests/
COPY api-server/ api-server/

# Build all workspace members except metrics (which has external dependency issues)
RUN cargo build \
    --package router-common \
    --package router-core \
    --package router-registry \
    --package router-access \
    --package router-middleware \
    --package router-timelock \
    --package router-multicall \
    --package router-quote \
    --package router-execution \
    --package router-api-server \
    2>&1

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
    "--package", "router-execution", \
    "--package", "router-api-server"]

# ── WASM build stage ──────────────────────────────────────────────────────────
FROM builder AS wasm
RUN cargo build --target wasm32-unknown-unknown --release \
    --package router-core \
    --package router-registry \
    --package router-access \
    --package router-middleware \
    --package router-timelock \
    --package router-multicall

# ── Metrics exporter runtime ──────────────────────────────────────────────────
FROM builder AS metrics-builder
RUN cargo build --release --package router-metrics-exporter

FROM debian:bookworm-slim AS metrics
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=metrics-builder /app/target/release/router-metrics-exporter /usr/local/bin/
EXPOSE 9090
ENTRYPOINT ["router-metrics-exporter"]
