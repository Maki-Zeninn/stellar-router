#!/usr/bin/env bash
#
# Load testing script for router-metrics-exporter using Artillery
#
# Prerequisites:
#   - Artillery installed (npm install -g artillery)
#   - router-metrics-exporter running on localhost:9090
#
# Usage:
#   ./scripts/load-test-artillery.sh [duration] [rps]
#
# Examples:
#   ./scripts/load-test-artillery.sh 30 100    # 30 seconds, 100 RPS
#   ./scripts/load-test-artillery.sh 60 500    # 60 seconds, 500 RPS

set -e

DURATION="${1:-30}"
RPS="${2:-100}"
BASE_URL="${BASE_URL:-http://localhost:9090}"
API_KEY="${ROUTER_API_KEY:-}"

echo "🚀 Starting load test for router-metrics-exporter (Artillery)"
echo "   Duration: ${DURATION}s"
echo "   Target RPS: $RPS"
echo "   Base URL: $BASE_URL"
echo ""

# Create temporary Artillery config
ARTILLERY_CONFIG=$(mktemp)
trap "rm -f $ARTILLERY_CONFIG" EXIT

cat > "$ARTILLERY_CONFIG" << EOF
config:
  target: "$BASE_URL"
  phases:
    - duration: 10
      arrivalRate: 1
      name: "Warm up"
    - duration: $DURATION
      arrivalRate: $RPS
      name: "Sustained load"
    - duration: 10
      arrivalRate: 1
      name: "Cool down"
  processor: "./scripts/artillery-processor.js"
  variables:
    api_key: "$API_KEY"
  http:
    timeout: 10

scenarios:
  - name: "Metrics Exporter Load Test"
    flow:
      - get:
          url: "/metrics"
          headers:
            X-Nonce: "{{ \$randomString(32) }}"
          expect:
            - statusCode: 200
      - think: 100
      - get:
          url: "/health"
          headers:
            X-Nonce: "{{ \$randomString(32) }}"
          expect:
            - statusCode: 200
      - think: 100
      - get:
          url: "/ready"
          headers:
            X-Nonce: "{{ \$randomString(32) }}"
          expect:
            - statusCode: [200, 503]
      - think: 100
EOF

# Run Artillery load test
artillery run "$ARTILLERY_CONFIG" --output /tmp/artillery-report.json

echo ""
echo "✅ Load test completed"
echo "📊 Report saved to /tmp/artillery-report.json"
echo ""
echo "View HTML report:"
echo "  artillery report /tmp/artillery-report.json"
