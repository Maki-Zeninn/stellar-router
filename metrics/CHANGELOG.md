# Changelog

All notable changes to the router-metrics-exporter will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-01-XX

### Added
- Initial release of the Prometheus/OpenTelemetry metrics exporter
- Support for scraping `router-core` contract metrics:
  - `router_core_total_routed` - cumulative route resolutions
  - `router_core_paused` - global pause status
  - `router_core_route_paused` - per-route pause status
- Support for scraping `router-middleware` contract metrics:
  - `router_middleware_total_calls` - cumulative pre-call invocations
  - `router_middleware_circuit_open` - circuit breaker open status
  - `router_middleware_failure_count` - consecutive failure count
- Support for scraping `router-registry` contract metrics:
  - `router_registry_total_names` - total registered contract names
- Exporter health metrics:
  - `router_scrape_duration_seconds` - histogram of scrape latency
  - `router_scrape_errors_total` - counter of failed scrapes
  - `router_up` - overall exporter health gauge
- HTTP server with `/metrics` and `/health` endpoints
- Configurable scrape interval (default 15s)
- Configurable RPC timeout (default 10s)
- Environment variable configuration support
- Docker support with multi-stage build
- Docker Compose setup with Prometheus and Grafana
- Comprehensive test suite (14 tests)
- Grafana dashboard JSON template
- CI/CD workflow for automated testing and builds

### Documentation
- Comprehensive README with usage examples
- Prometheus configuration examples
- Grafana query examples
- Troubleshooting guide
- OpenTelemetry integration guide
- Docker deployment guide

### Known Limitations
- XDR transaction building not implemented (uses simulation-based scraping)
- No real-time event streaming (poll-based only)
- No transaction-level latency tracking (scrape latency only)

## [Unreleased]

### Planned
- Support for custom metric labels via configuration
- Support for scraping `router-access` contract metrics (role counts, blacklist size)
- Support for scraping `router-timelock` contract metrics (queued operations, execution delays)
- Support for scraping `router-multicall` contract metrics (batch sizes, success rates)
- Alerting rule templates for Prometheus
- Helm chart for Kubernetes deployment
- Integration with Stellar Horizon for transaction-level metrics
- Real-time event streaming via Stellar SSE
- Proper XDR encoding/decoding using `stellar-xdr` crate
- Metric cardinality limits to prevent label explosion
- Support for multiple network endpoints (failover)
- Metric aggregation across multiple contract instances
