# Issue #1668 — `HighScrapeDuration` Runs `histogram_quantile` on Raw Cumulative Buckets Without `rate()` or `by (le)`

**Tracking:** [#1668](https://github.com/Maki-Zeninn/stellar-router/issues/1668)  
**Component:** `metrics/alerts.yml`, `metrics/dashboards/stellar-router.json`  
**Type:** Bug  
**Impact:** Alert p95 reflects all-time history rather than recent latency; slow periods from hours ago suppress detection of new regressions

---

## Summary

The `HighScrapeDuration` alert computes a p95 latency using `histogram_quantile` applied
directly to the raw cumulative bucket counter. Without `rate()`, the quantile is
calculated over every observation since the exporter process started. A latency spike
that occurred hours or days ago continues to inflate p95 indefinitely, while a new spike
barely moves the needle.

---

## Location

**File:** `metrics/alerts.yml`  
**Group:** `stellar-router-alerts`

```yaml
- alert: HighScrapeDuration
  expr: histogram_quantile(0.95, router_scrape_duration_seconds_bucket) > 5
  for: 2m
  labels:
    severity: warning
    component: exporter
  annotations:
    summary: "High p95 scrape latency for contract {{ $labels.contract }}"
    description: "P95 scrape latency for {{ $labels.contract }} is {{ $value }}s. RPC endpoint may be slow or overloaded."
```

---

## Root Cause

Prometheus histogram metrics (`_bucket`, `_sum`, `_count`) are monotonically increasing
counters. `histogram_quantile` requires a **rate of observations per second** as input,
not raw cumulative counts. The standard pattern is:

```
histogram_quantile(φ, sum by (le, <labels>) (rate(metric_bucket[window])))
```

When `rate()` is omitted:

1. The quantile is computed over the entire lifetime of the counter, not the recent
   window. A single slow scrape from hours ago remains in the cumulative distribution
   and keeps p95 elevated long after the problem is resolved.
2. Conversely, a new slowdown produces only a small incremental change relative to the
   accumulated total, so it may not push p95 above the threshold for many minutes.
3. The `by (le)` aggregation is also missing. `histogram_quantile` requires the `le`
   label to be preserved. Without an explicit aggregation, label cardinality issues can
   cause incorrect bucket merging across contracts.

---

## Impact

- The alert may stay firing after a latency issue has been fully resolved (false
  positive), causing alert fatigue.
- A genuine new latency regression may go undetected for far longer than the `for: 2m`
  window suggests (false negative).
- Any Grafana panel in `metrics/dashboards/stellar-router.json` that uses the same
  expression has the same accuracy problem.

---

## Required Change

**`metrics/alerts.yml` — fix the `HighScrapeDuration` expression:**

```yaml
- alert: HighScrapeDuration
  expr: >
    histogram_quantile(
      0.95,
      sum by (le, contract) (
        rate(router_scrape_duration_seconds_bucket[5m])
      )
    ) > 5
  for: 2m
  labels:
    severity: warning
    component: exporter
  annotations:
    summary: "High p95 scrape latency for contract {{ $labels.contract }}"
    description: >
      P95 scrape latency for {{ $labels.contract }} is {{ $value }}s over the
      last 5 minutes. The RPC endpoint may be slow or overloaded.
```

Key changes:
- `rate(...[5m])` converts cumulative buckets into a per-second rate over a recent
  5-minute window.
- `sum by (le, contract)` preserves the `le` label (required by `histogram_quantile`)
  and aggregates by `contract` so each contract gets its own quantile.

**`metrics/dashboards/stellar-router.json` — audit any panel using `router_scrape_duration_seconds_bucket`.**

Apply the same `rate()` + `sum by (le, contract)` pattern to every panel that calls
`histogram_quantile` on this metric.

---

## Acceptance Criteria

- [ ] The `HighScrapeDuration` alert expression uses `rate(router_scrape_duration_seconds_bucket[5m])`.
- [ ] The expression aggregates `by (le, contract)` before passing to `histogram_quantile`.
- [ ] Any Grafana panel in `metrics/dashboards/stellar-router.json` using the same metric applies the same fix.
- [ ] `promtool check rules metrics/alerts.yml` passes without warnings.
