#!/usr/bin/env bash
#
# Load testing script for router-metrics-exporter using k6
#
# Prerequisites:
#   - k6 installed (https://k6.io/docs/getting-started/installation/)
#   - router-metrics-exporter running on localhost:9090
#
# Usage:
#   ./scripts/load-test.sh [duration] [vus] [rps]
#
# Examples:
#   ./scripts/load-test.sh 30s 10 100    # 30 seconds, 10 VUs, 100 RPS
#   ./scripts/load-test.sh 1m 50 500     # 1 minute, 50 VUs, 500 RPS
#   ./scripts/load-test.sh 5m 100 1000   # 5 minutes, 100 VUs, 1000 RPS

set -e

DURATION="${1:-30s}"
VUS="${2:-10}"
RPS="${3:-100}"
BASE_URL="${BASE_URL:-http://localhost:9090}"
API_KEY="${ROUTER_API_KEY:-}"

echo "🚀 Starting load test for router-metrics-exporter"
echo "   Duration: $DURATION"
echo "   Virtual Users: $VUS"
echo "   Target RPS: $RPS"
echo "   Base URL: $BASE_URL"
echo ""

# Create temporary k6 script
K6_SCRIPT=$(mktemp)
trap "rm -f $K6_SCRIPT" EXIT

cat > "$K6_SCRIPT" << 'EOF'
import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter, Gauge } from 'k6/metrics';

// Custom metrics
const errorRate = new Rate('errors');
const metricsLatency = new Trend('metrics_latency');
const healthLatency = new Trend('health_latency');
const readyLatency = new Trend('ready_latency');
const requestCount = new Counter('requests');
const activeVUs = new Gauge('active_vus');

export const options = {
  stages: [
    { duration: '10s', target: __ENV.VUS },      // Ramp up
    { duration: __ENV.DURATION, target: __ENV.VUS }, // Stay at target
    { duration: '10s', target: 0 },              // Ramp down
  ],
  thresholds: {
    'http_req_duration': ['p(95)<500', 'p(99)<1000'],
    'errors': ['rate<0.1'],
  },
};

export default function () {
  activeVUs.add(1);

  const headers = {
    'Content-Type': 'application/json',
  };

  // Add API key if provided
  if (__ENV.API_KEY) {
    headers['X-API-Key'] = __ENV.API_KEY;
  }

  // Add nonce for replay protection
  const nonce = `nonce-${Date.now()}-${Math.random()}`;
  headers['X-Nonce'] = nonce;

  // Test /metrics endpoint
  let res = http.get(`${__ENV.BASE_URL}/metrics`, { headers });
  metricsLatency.add(res.timings.duration);
  errorRate.add(res.status !== 200);
  requestCount.add(1);
  check(res, {
    'metrics status is 200': (r) => r.status === 200,
    'metrics response time < 500ms': (r) => r.timings.duration < 500,
  });

  sleep(0.1);

  // Test /health endpoint
  res = http.get(`${__ENV.BASE_URL}/health`, { headers });
  healthLatency.add(res.timings.duration);
  errorRate.add(res.status !== 200);
  requestCount.add(1);
  check(res, {
    'health status is 200': (r) => r.status === 200,
  });

  sleep(0.1);

  // Test /ready endpoint
  res = http.get(`${__ENV.BASE_URL}/ready`, { headers });
  readyLatency.add(res.timings.duration);
  errorRate.add(res.status !== 200 && res.status !== 503);
  requestCount.add(1);
  check(res, {
    'ready status is 200 or 503': (r) => r.status === 200 || r.status === 503,
  });

  sleep(0.1);

  activeVUs.add(-1);
}
EOF

# Run k6 load test
k6 run \
  --vus "$VUS" \
  --duration "$DURATION" \
  --rps "$RPS" \
  -e "VUS=$VUS" \
  -e "DURATION=$DURATION" \
  -e "BASE_URL=$BASE_URL" \
  -e "API_KEY=$API_KEY" \
  "$K6_SCRIPT"

echo ""
echo "✅ Load test completed"
