# Issue #1669 — `alerts.yml` Has Duplicate Alerts, Misnamed Alerts, and `increase()` Applied to Gauges

**Tracking:** [#1669](https://github.com/Maki-Zeninn/stellar-router/issues/1669)  
**Component:** `metrics/alerts.yml`  
**Type:** Refactor / Bug  
**Impact:** Every incident pages twice; two alerts fire on the wrong condition; `increase()` on gauges produces incorrect results

---

## Summary

`metrics/alerts.yml` contains two rule groups — `stellar-router.rules` and
`stellar-router-alerts` — that partially overlap. Several alert conditions are defined
twice under different names. Additionally, two alerts have misleading names that do not
match what they actually check, and two alerts misuse `increase()` on gauge metrics,
producing unreliable threshold comparisons.

---

## Location

**File:** `metrics/alerts.yml`

---

## Problems

### 1. Duplicate alerts across two groups

The following conditions are defined in both groups:

| Condition | Group 1 (`stellar-router.rules`) | Group 2 (`stellar-router-alerts`) |
|---|---|---|
| Circuit breaker open | `RouterMiddlewareCircuitBreakerOpen` | `CircuitBreakerOpen` |
| Scrape errors present | `RouterMiddlewareHighErrorRate` | `MetricsScraperFailing` |
| High failure count | `RouterMiddlewareHighFailureRate` | `HighConsecutiveFailures` |

When any of these conditions is true, both groups evaluate independently and each fires
its own alert. Operators receive two pages for a single incident.

### 2. Misnamed alerts

**`RouterMiddlewareHighErrorRate`** is named after the middleware but its expression
checks `router_scrape_errors_total`, which is an exporter/scraper metric with no
relation to middleware processing:

```yaml
- alert: RouterMiddlewareHighErrorRate
  expr: increase(router_scrape_errors_total[5m]) > 0
```

The name implies middleware errors; the metric measures exporter scrape failures.

**`RouterMiddlewareRateLimitViolations`** fires on total call volume, not on
rate-limit violations:

```yaml
- alert: RouterMiddlewareRateLimitViolations
  expr: increase(router_middleware_total_calls[5m]) > 10000
```

A high call count is not the same as a rate-limit violation. The alert name misleads
responders into investigating a rate-limit issue when the root cause may simply be
legitimate high traffic.

### 3. `increase()` applied to gauges

`router_middleware_failure_count` and `router_middleware_total_calls` are **gauges**.
They reset to zero when a failure sequence ends (e.g. when a circuit breaker recovers).
`increase()` is defined for monotonically increasing counters: it assumes that any
decrease is a counter reset and compensates by extrapolating upward. Applied to a gauge
that legitimately drops to zero on recovery, this produces an inflated or nonsensical
result:

```yaml
# From stellar-router.rules — both are wrong for gauges
- alert: RouterMiddlewareHighFailureRate
  expr: increase(router_middleware_failure_count[5m]) > 10

- alert: RouterMiddlewareHighErrorRate
  expr: increase(router_middleware_total_calls[5m]) > 10000  # also wrong label claim
```

The `HighConsecutiveFailures` alert in `stellar-router-alerts` correctly queries the
gauge directly (`router_middleware_failure_count > 10`) without `increase()`.

The `{job!=""}` label selector used in `RouterMiddlewareCircuitBreakerOpen` filters
nothing in practice — every scraped metric has a non-empty `job` label — and adds noise
to the expression.

---

## Required Change

Merge `stellar-router.rules` and `stellar-router-alerts` into a single group. Keep one
alert per condition, using the most precise expression from either group. Apply these
specific fixes:

**Circuit breaker:** Keep `CircuitBreakerOpen` and `CircuitBreakerStuck` from group 2.
Remove `RouterMiddlewareCircuitBreakerOpen` from group 1 (and drop the `{job!=""}` filter).

**Failure count:** Keep `HighConsecutiveFailures` and `CriticalFailureCount` from group 2
(both query the gauge directly). Remove `RouterMiddlewareHighFailureRate` from group 1.

**Scrape errors:** Keep `MetricsScraperFailing` from group 2. Remove
`RouterMiddlewareHighErrorRate` from group 1. Consider renaming `MetricsScraperFailing`
to `ExporterScrapeErrors` to make its scope clearer.

**Rate-limit alert:** Remove or correct `RouterMiddlewareRateLimitViolations`. It fires on
total call volume, not violations. If a high-volume alert is still desired, rename it
(e.g. `HighTotalCallVolume`) and update its description to match.

**Gauges — replace `increase()` with direct comparison or `delta()`/`max_over_time()`:**

| Use case | Correct approach |
|---|---|
| "Gauge exceeded threshold in window" | Direct: `metric > threshold` |
| "Gauge increased significantly" | `delta(metric[5m]) > threshold` |
| "Gauge peaked above threshold" | `max_over_time(metric[5m]) > threshold` |

---

## Acceptance Criteria

- [ ] `metrics/alerts.yml` contains exactly one rule group.
- [ ] No alert condition is defined more than once.
- [ ] No alert uses `increase()` or `rate()` on a gauge metric.
- [ ] Alert names match what the expression actually checks.
- [ ] `promtool check rules metrics/alerts.yml` passes without warnings.
