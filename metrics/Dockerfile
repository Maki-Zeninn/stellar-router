# Multi-stage build for minimal final image
FROM rust:1.88-slim AS builder

# curl is required by utoipa-swagger-ui's build script, which downloads the
# Swagger UI release archive at compile time.
RUN apt-get update && \
    apt-get install -y --no-install-recommends curl ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy dependency files first for better layer caching
COPY metrics/Cargo.toml ./

# Create a dummy main.rs to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code
COPY metrics/src/ src/

# Build the actual binary
RUN touch src/main.rs && \
    cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install CA certificates for HTTPS RPC calls
RUN apt-get update && \
    apt-get install -y ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /build/target/release/router-metrics-exporter /usr/local/bin/

# Create non-root user
RUN useradd -m -u 1000 metrics && \
    chown -R metrics:metrics /usr/local/bin/router-metrics-exporter

USER metrics

EXPOSE 9090

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -f http://localhost:9090/health || exit 1

ENTRYPOINT ["router-metrics-exporter"]
