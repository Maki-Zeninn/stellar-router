# syntax=docker/dockerfile:1
# ── Build stage ───────────────────────────────────────────────────────────────
FROM rust:1.88-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends curl && rm -rf /var/lib/apt/lists/*
RUN rustup target add wasm32-unknown-unknown

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY contracts/ contracts/
COPY metrics/ metrics/

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
    --package router-metrics-exporter

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
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/router-metrics-exporter /usr/local/bin/
RUN useradd -m -u 1000 metrics && \
    chown -R metrics:metrics /usr/local/bin/router-metrics-exporter
USER metrics
EXPOSE 9090
ENTRYPOINT ["router-metrics-exporter"]
