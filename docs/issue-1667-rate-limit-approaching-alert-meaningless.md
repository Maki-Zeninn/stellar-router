# Issue #1667 — `RateLimitApproaching` Alert Expression Is Meaningless and Route Label Is Always Empty

**Tracking:** [#1667](https://github.com/Maki-Zeninn/stellar-router/issues/1667)  
**Component:** `metrics/alerts.yml`  
**Type:** Bug  
**Impact:** Alert never fires meaningfully; `{{ $labels.route }}` is always blank in notifications

---

## Summary

The `RateLimitApproaching` alert in the `stellar-router-alerts` group compares a
per-second call rate against a value derived from `scalar(count(...) by (route))`.
Both the label used for grouping and the threshold being compared are wrong, so the
alert never correctly represents proximity to any configured rate limit.

---

## Location

**File:** `metrics/alerts.yml`  
**Group:** `stellar-router-alerts`

```yaml
- alert: RateLimitApproaching
  expr: |
    (rate(router_middleware_total_calls[5m]) / 60) > 0.8 *
    scalar(count(router_middleware_total_calls) by (route))
  for: 2m
  labels:
    severity: warning
    component: middleware
  annotations:
    summary: "Rate limit approaching for route {{ $labels.route }}"
    description: "Route {{ $labels.route }} is at 80% of its configured rate limit. Requests may start being rejected soon."
```

---

## Root Cause

There are two independent bugs in this expression.

**1. `router_middleware_total_calls` has no `route` label.**

The metric only carries a `contract` label. `count(...) by (route)` therefore groups
everything into a single series whose `route` label value is an empty string. As a
result, `{{ $labels.route }}` in every alert notification is blank.

**2. The threshold has no connection to any rate-limit configuration.**

`scalar(count(router_middleware_total_calls) by (route))` returns the number of distinct
time series for `router_middleware_total_calls` grouped by `route` — in a typical
deployment this is 1 (one contract). The expression therefore asks:

> Is `(calls/second / 60) > 0.8 * 1`?

That is equivalent to asking whether average call rate exceeds about 48 calls per
minute, which is unrelated to any per-route limit configured inside the contract.

The metrics exporter does not expose per-route rate-limit configuration or the number of
calls within the current rate-limit window. The data needed to compute "80% of the
configured rate limit" does not exist in the metric set.

---

## Impact

- The alert may fire spuriously on any contract receiving more than ~48 calls/minute,
  regardless of its actual configured limit.
- Notifications always show an empty route label, making them unactionable.
- `promtool check rules` does not catch this because the PromQL is syntactically valid.
- Operators may either silence the alert (losing real signal) or investigate false
  positives repeatedly.

---

## Required Change

**Option A — Remove the alert (preferred until the exporter exposes the right data).**

Delete the `RateLimitApproaching` block from `metrics/alerts.yml`. An alert that cannot
compute what it claims to compute provides negative value.

**Option B — Add proper per-route metrics to the exporter, then alert on them.**

Expose two new gauge metrics from the exporter by calling `get_route_rate_limit_stats`:

| Metric | Description |
|---|---|
| `router_middleware_rate_limit_max{contract, route}` | Configured call limit for the window |
| `router_middleware_rate_limit_used{contract, route}` | Calls consumed in the current window |

Then rewrite the alert:

```yaml
- alert: RateLimitApproaching
  expr: |
    (router_middleware_rate_limit_used / router_middleware_rate_limit_max) > 0.8
  for: 2m
  labels:
    severity: warning
    component: middleware
  annotations:
    summary: "Rate limit approaching for route {{ $labels.route }}"
    description: >
      Route {{ $labels.route }} on contract {{ $labels.contract }} has used
      {{ $value | humanizePercentage }} of its configured rate limit.
```

---

## Acceptance Criteria

- [ ] No alert in `metrics/alerts.yml` claims to track rate-limit proximity unless the
      exporter exposes per-route limit configuration and usage metrics.
- [ ] If the alert is removed, the file no longer contains a `RateLimitApproaching` rule.
- [ ] If the alert is kept, the expression uses only labels that the referenced metric
      actually carries, and `{{ $labels.route }}` resolves to a non-empty value.
- [ ] `promtool check rules metrics/alerts.yml` passes without warnings.
