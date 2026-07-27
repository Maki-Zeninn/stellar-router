# Quick Start Guide

Get the stellar-router metrics exporter running in 5 minutes.

## Prerequisites

- Rust 1.83+ (for building from source)
- OR Docker + Docker Compose (for containerized deployment)
- Deployed stellar-router contracts on Stellar testnet or mainnet

## Option 1: Run Locally (Fastest)

### Step 1: Get Contract IDs

After deploying your router contracts, note their contract IDs:

```bash
# Example contract IDs (replace with yours)
CORE_ID="CBGTG4XVLSQFXK7JXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQ"
MIDDLEWARE_ID="CBGTG4XVLSQFXK7JXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQ"
REGISTRY_ID="CBGTG4XVLSQFXK7JXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQXQ"
```

### Step 2: Build the Exporter

```bash
cd metrics
cargo build --release
```

### Step 3: Run the Exporter

```bash
./target/release/router-metrics-exporter \
  --rpc-url "https://soroban-testnet.stellar.org" \
  --core-contract-id "$CORE_ID" \
  --middleware-contract-id "$MIDDLEWARE_ID" \
  --registry-contract-id "$REGISTRY_ID" \
  --scrape-interval-secs 30
```

### Step 4: View Metrics

```bash
# In another terminal
curl http://localhost:9090/metrics

# You should see output like:
# router_core_total_routed{contract="CBGTG..."} 42
# router_middleware_total_calls{contract="CBGTG..."} 128
# router_up 1
```

✅ **Done!** The exporter is now running and exposing metrics.

---

## Option 2: Docker Compose (Full Stack)

This option includes Prometheus and Grafana for visualization.

### Step 1: Configure Environment

```bash
cd metrics
cp .env.example .env

# Edit .env with your contract IDs
nano .env
```

Example `.env`:

```bash
ROUTER_RPC_URL=https://soroban-testnet.stellar.org
ROUTER_CORE_CONTRACT_ID=CBGTG4XVLSQFXK7J...
ROUTER_MIDDLEWARE_CONTRACT_ID=CBGTG4XVLSQFXK7J...
ROUTER_REGISTRY_CONTRACT_ID=CBGTG4XVLSQFXK7J...
ROUTER_SCRAPE_INTERVAL_SECS=30
```

### Step 2: Start the Stack

```bash
docker-compose up -d
```

This starts:
- **router-metrics-exporter** on port 9090
- **Prometheus** on port 9091
- **Grafana** on port 3000

### Step 3: Access Services

| Service | URL | Credentials |
|---------|-----|-------------|
| Metrics | http://localhost:9090/metrics | — |
| Health | http://localhost:9090/health | — |
| Ready | http://localhost:9090/ready | — |
| Prometheus | http://localhost:9091 | — |
| Grafana | http://localhost:3000 | admin / admin |

### Step 4: Import Grafana Dashboard

1. Open Grafana at http://localhost:3000
2. Login with `admin` / `admin`
3. Go to **Dashboards** → **Import**
4. Upload `grafana-dashboard.json`
5. Select **Prometheus** as the data source
6. Click **Import**

✅ **Done!** You now have a full monitoring stack.

---

## Option 3: Docker Only (No Compose)

### Step 1: Build the Image

```bash
docker build -t stellar-router/metrics-exporter -f metrics/Dockerfile .
```

### Step 2: Run the Container

```bash
docker run -d \
  --name router-metrics \
  -p 9090:9090 \
  -e ROUTER_RPC_URL="https://soroban-testnet.stellar.org" \
  -e ROUTER_CORE_CONTRACT_ID="CBGTG..." \
  -e ROUTER_MIDDLEWARE_CONTRACT_ID="CBGTG..." \
  -e ROUTER_REGISTRY_CONTRACT_ID="CBGTG..." \
  -e ROUTER_SCRAPE_INTERVAL_SECS=30 \
  stellar-router/metrics-exporter
```

### Step 3: View Logs

```bash
docker logs -f router-metrics
```

### Step 4: View Metrics

```bash
curl http://localhost:9090/metrics
```

✅ **Done!** The exporter is running in a container.

---

## Verify It's Working

### Check Health Endpoint

```bash
curl http://localhost:9090/health
# Expected: "ok"
```

### Check Metrics Endpoint

```bash
curl http://localhost:9090/metrics | grep router_up
# Expected: router_up 1
```

### Check Logs

```bash
# Local build
RUST_LOG=router_metrics_exporter=info ./target/release/router-metrics-exporter

# Docker
docker logs router-metrics

# Expected log output:
# INFO router_metrics_exporter: router-metrics-exporter starting
# INFO router_metrics_exporter::collector: scrape loop started
# INFO router_metrics_exporter::collector: scraping router-core
# INFO router_metrics_exporter::collector: core scrape done
```

---

## Next Steps

### Add to Prometheus

Edit your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'stellar-router'
    scrape_interval: 30s
    static_configs:
      - targets: ['localhost:9090']
```

Reload Prometheus:

```bash
curl -X POST http://localhost:9091/-/reload
```

### Create Alerts

Example alert rule (`alerts.yml`):

```yaml
groups:
  - name: stellar-router
    interval: 30s
    rules:
      - alert: RouterDown
        expr: router_up == 0
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Router metrics exporter is down"

      - alert: CircuitBreakerOpen
        expr: router_middleware_circuit_open == 1
        for: 2m
        labels:
          severity: warning
        annotations:
          summary: "Circuit breaker open for route {{ $labels.route }}"

      - alert: HighScrapeLatency
        expr: histogram_quantile(0.95, rate(router_scrape_duration_seconds_bucket[5m])) > 5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High scrape latency (p95 > 5s)"
```

### Explore Metrics in Grafana

1. Open Grafana at http://localhost:3000
2. Go to **Explore**
3. Select **Prometheus** data source
4. Try these queries:

```promql
# Route resolution rate (per minute)
rate(router_core_total_routed[5m]) * 60

# Circuit breaker status
router_middleware_circuit_open

# Scrape latency (p95)
histogram_quantile(0.95, rate(router_scrape_duration_seconds_bucket[5m]))
```

---

## Troubleshooting

### "Connection refused" when accessing metrics

**Cause**: Exporter not running or wrong port.

**Fix**:
```bash
# Check if process is running
ps aux | grep router-metrics-exporter

# Check if port is listening
lsof -i :9090

# Check logs
docker logs router-metrics  # if using Docker
```

### "router_up" is 0

**Cause**: RPC scrape failed.

**Fix**:
```bash
# Enable debug logging
RUST_LOG=router_metrics_exporter=debug ./target/release/router-metrics-exporter

# Check RPC endpoint is reachable
curl https://soroban-testnet.stellar.org

# Verify contract IDs are correct
stellar contract invoke --id <CONTRACT_ID> --network testnet -- total_routed
```

### High scrape latency

**Cause**: Too many routes or slow RPC endpoint.

**Fix**:
```bash
# Increase scrape interval
--scrape-interval-secs 60

# Increase RPC timeout
--rpc-timeout-secs 20

# Use a dedicated RPC endpoint (not public)
--rpc-url "https://your-private-rpc.example.com"
```

---

## Configuration Reference

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--rpc-url` | `ROUTER_RPC_URL` | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint |
| `--core-contract-id` | `ROUTER_CORE_CONTRACT_ID` | `` | router-core contract ID |
| `--middleware-contract-id` | `ROUTER_MIDDLEWARE_CONTRACT_ID` | `` | router-middleware contract ID |
| `--registry-contract-id` | `ROUTER_REGISTRY_CONTRACT_ID` | `` | router-registry contract ID |
| `--scrape-interval-secs` | `ROUTER_SCRAPE_INTERVAL_SECS` | `15` | Scrape interval (seconds) |
| `--listen` | `ROUTER_LISTEN` | `0.0.0.0:9090` | HTTP listen address |
| `--rpc-timeout-secs` | `ROUTER_RPC_TIMEOUT_SECS` | `10` | RPC timeout (seconds) |

---

## Support

- **Documentation**: See [README.md](README.md) for full details
- **Issues**: Open an issue on GitHub
- **Logs**: Run with `RUST_LOG=router_metrics_exporter=debug` for verbose output

---

**Happy monitoring! 🚀**
