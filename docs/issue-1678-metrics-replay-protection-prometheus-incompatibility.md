# Issue 1678 — Replay Protection Breaks Prometheus Scraping

**Tracks:** [#1678](https://github.com/Maki-Zeninn/stellar-router/issues/1678)  
**Affects:** `metrics/README.md`, `metrics/src/replay_protection.rs`

---

## Problem

When `ROUTER_REPLAY_PROTECTION_ENABLED=true`, the `/metrics` endpoint requires
every HTTP request to carry a unique `X-Nonce` header. Requests that omit the
header receive `400 Bad Request`; requests that reuse a nonce (within
`ROUTER_NONCE_TTL_SECS`) receive `409 Conflict`.

Prometheus cannot satisfy this requirement. Prometheus scrape configurations
support static headers only — there is no mechanism for generating a unique
nonce per scrape. After the first successful scrape, every subsequent scrape
reuses the same nonce and gets `409`. The `router_up` gauge drops to `0` and
all series go stale, triggering `MetricsExporterDown` alerts.

This matters because `docs/deployment.md` and `docs/security.md` both recommend
enabling replay protection for production, but doing so silently breaks the
primary consumer of `/metrics`.

Additionally, `metrics/README.md` documents `ROUTER_AUTH_ENABLED` and
`ROUTER_API_KEY` but does not mention `ROUTER_REPLAY_PROTECTION_ENABLED`,
`ROUTER_NONCE_CACHE_SIZE`, or `ROUTER_NONCE_TTL_SECS`.

---

## Recommended Resolution

Replay protection provides no security benefit on an idempotent, read-only
`GET /metrics` endpoint. The `/metrics` path should be excluded from the
nonce check in `replay_protection.rs`. Alternatively, if the behaviour is
intentional (e.g. for custom clients that do supply nonces), the
incompatibility with Prometheus must be documented clearly.

Whichever path is taken, all three environment variables must be added to the
README configuration table.

---

## Required Changes to `metrics/README.md`

### Add replay-protection variables to the configuration table

Add a **Replay Protection** section to the Authentication section (or extend
the existing configuration table):

#### Environment Variables — Replay Protection

| Variable | Default | Description |
|---|---|---|
| `ROUTER_REPLAY_PROTECTION_ENABLED` | `false` | Enable nonce-based replay detection on HTTP endpoints |
| `ROUTER_NONCE_CACHE_SIZE` | `1024` | Maximum number of nonces held in memory before oldest entries are evicted |
| `ROUTER_NONCE_TTL_SECS` | `300` | How long (in seconds) a used nonce is remembered and rejected if resubmitted |

### Add a Replay Protection section

Insert this section after the existing **Authentication** section:

---

### Replay Protection

The exporter optionally enforces nonce-based replay detection to prevent
identical HTTP requests from being replayed.

> **⚠ Incompatibility with Prometheus:** When `ROUTER_REPLAY_PROTECTION_ENABLED=true`,
> every request to `/metrics` must include a unique `X-Nonce` header. Prometheus
> scrape configurations support only static headers and cannot generate a fresh
> nonce per scrape. **Do not enable replay protection if you are scraping with
> a stock Prometheus setup.** Doing so will cause every scrape after the first
> to return `409 Conflict`, making `router_up` go to `0` and all series go stale.

#### When to enable

Replay protection is designed for custom clients (dashboards, scripts, CI jobs)
that you control and can program to supply a fresh nonce on each request. It is
not useful for the `/metrics` read path.

#### Enabling replay protection

```bash
export ROUTER_REPLAY_PROTECTION_ENABLED=true
export ROUTER_NONCE_CACHE_SIZE=2048   # optional, increase for high-throughput clients
export ROUTER_NONCE_TTL_SECS=60       # optional, shorten TTL to reduce memory pressure
```

#### Making a request with a nonce

```bash
# Generate a unique nonce (UUID v4 is sufficient)
NONCE=$(uuidgen | tr '[:upper:]' '[:lower:]')

curl -H "X-Nonce: $NONCE" \
     -H "Authorization: Bearer your-secret-api-key" \
     http://localhost:9090/metrics
```

Reusing the same nonce within the TTL window returns `409 Conflict`.

#### Prometheus configuration when replay protection is off (recommended)

A standard Prometheus scrape config works when replay protection is disabled
(the default):

```yaml
scrape_configs:
  - job_name: 'stellar-router'
    scrape_interval: 30s
    authorization:
      credentials: 'your-secret-api-key'
    static_configs:
      - targets: ['localhost:9090']
        labels:
          environment: 'testnet'
          service: 'stellar-router'
```

---

## Required Change to `metrics/src/replay_protection.rs`

The `/metrics` path should be excluded from the nonce check. The check should
apply only to state-mutating endpoints (if any are added in future). A minimal
implementation:

```rust
// Skip replay protection for read-only GET /metrics
if req.method() == Method::GET && req.uri().path() == "/metrics" {
    return next.run(req).await;
}
```

If the decision is made to keep the check on `/metrics` (e.g. to support
custom clients), the exclusion approach must be documented as a deliberate
trade-off and the incompatibility note above must remain in the README.

---

## Acceptance Criteria

- [ ] A stock Prometheus config can scrape the exporter with every documented option
      enabled, **or** the incompatibility is documented with a clear warning
- [ ] `ROUTER_REPLAY_PROTECTION_ENABLED` is in the README configuration table
- [ ] `ROUTER_NONCE_CACHE_SIZE` is in the README configuration table
- [ ] `ROUTER_NONCE_TTL_SECS` is in the README configuration table
- [ ] Decision on whether `/metrics` is excluded from nonce-checking is recorded
      in code comments or this document
