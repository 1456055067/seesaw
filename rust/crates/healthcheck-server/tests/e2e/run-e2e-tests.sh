#!/usr/bin/env bash
# E2E Test Runner for healthcheck-server
#
# Builds and launches a containerized stack (healthcheck-server, nginx target,
# proxy-simulator), then validates the full healthcheck lifecycle:
#   1. Server startup and ready message
#   2. Config delivery and healthy state detection
#   3. Metrics endpoint
#   4. Failure detection (target goes unhealthy)
#   5. Recovery detection (target restored)
#
# Usage: ./run-e2e-tests.sh
# Prerequisites: docker compose v2+

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.e2e.yml"
PROJECT_NAME="hc-e2e"

# Disable Docker buildx filesystem entitlements check (build context is local)
export BUILDX_BAKE_ENTITLEMENTS_FS=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0
FAIL=0
TOTAL=0

pass() { TOTAL=$((TOTAL+1)); PASS=$((PASS+1)); echo -e "${GREEN}[PASS]${NC} $1"; }
fail() { TOTAL=$((TOTAL+1)); FAIL=$((FAIL+1)); echo -e "${RED}[FAIL]${NC} $1"; }
info() { echo -e "${YELLOW}[INFO]${NC} $1"; }

compose() {
    DOCKER_BUILDKIT=1 BUILDX_BAKE_ENTITLEMENTS_FS=0 \
        docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" "$@"
}

cleanup() {
    info "Cleaning up containers..."
    compose down -v --remove-orphans 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# --------------------------------------------------------
# Build and start
# --------------------------------------------------------
info "Building containers (this may take a few minutes on first run)..."
compose build

info "Starting containers..."
compose up -d

# --------------------------------------------------------
# Wait for proxy-simulator to connect and send configs
# --------------------------------------------------------
info "Waiting for proxy-simulator to connect and send configs..."
for i in $(seq 1 90); do
    if compose logs proxy-simulator 2>/dev/null | grep -q "CONFIG_SENT"; then
        break
    fi
    sleep 1
done

if compose logs proxy-simulator 2>/dev/null | grep -q "CONFIG_SENT"; then
    pass "Proxy simulator connected and sent configs"
else
    fail "Proxy simulator did not send configs within 90s"
    echo ""
    info "=== proxy-simulator logs ==="
    compose logs proxy-simulator 2>/dev/null || true
    info "=== healthcheck-server logs ==="
    compose logs healthcheck-server 2>/dev/null || true
    exit 1
fi

# --------------------------------------------------------
# TEST 1: Ready message
# --------------------------------------------------------
info "--- Test 1: Ready Message ---"
if compose logs proxy-simulator 2>/dev/null | grep -q '"type":"ready"'; then
    pass "Server sent ready message"
else
    fail "No ready message in proxy-simulator logs"
fi

# --------------------------------------------------------
# TEST 2: Healthy notifications
# --------------------------------------------------------
info "--- Test 2: Healthy Notifications ---"
info "Waiting up to 30s for healthy state notifications..."

GOT_HEALTHY=false
for i in $(seq 1 30); do
    if compose logs proxy-simulator 2>/dev/null | grep "RECV:" | grep "notification_batch" | grep -q '"state":"healthy"'; then
        GOT_HEALTHY=true
        break
    fi
    sleep 1
done

if $GOT_HEALTHY; then
    pass "Received healthy state notification"
else
    fail "No healthy notification within 30s"
fi

# --------------------------------------------------------
# TEST 3: Metrics endpoint
# --------------------------------------------------------
info "--- Test 3: Metrics Endpoint ---"

METRICS=$(compose exec -T proxy-simulator curl -sf http://healthcheck-server:9090/metrics 2>/dev/null || echo "")

if [ -n "$METRICS" ]; then
    pass "Metrics endpoint accessible"

    for M in healthcheck_monitors_active healthcheck_checks_total healthcheck_proxy_connected healthcheck_state; do
        if echo "$METRICS" | grep -q "$M"; then
            pass "Metric present: $M"
        else
            fail "Metric missing: $M"
        fi
    done
else
    fail "Cannot reach metrics endpoint"
fi

# --------------------------------------------------------
# TEST 4: Failure detection
# --------------------------------------------------------
info "--- Test 4: Failure Detection ---"

# Count current unhealthy notifications before triggering failure
UNHEALTHY_COUNT_BEFORE=$(compose logs proxy-simulator 2>/dev/null | grep "RECV:" | grep "notification_batch" | grep -c '"state":"unhealthy"' || true)

info "Removing healthy.txt from target-service (HTTP /health will return 503)..."
compose exec -T target-service rm -f /usr/share/nginx/html/healthy.txt

info "Waiting up to 45s for NEW unhealthy notification on HTTP check..."

GOT_UNHEALTHY=false
for i in $(seq 1 45); do
    UNHEALTHY_COUNT=$(compose logs proxy-simulator 2>/dev/null | grep "RECV:" | grep "notification_batch" | grep -c '"state":"unhealthy"' || true)
    if [ "$UNHEALTHY_COUNT" -gt "$UNHEALTHY_COUNT_BEFORE" ]; then
        GOT_UNHEALTHY=true
        break
    fi
    sleep 1
done

if $GOT_UNHEALTHY; then
    pass "Detected unhealthy state (HTTP check)"
else
    fail "No unhealthy notification within 45s"
fi

# --------------------------------------------------------
# TEST 5: Recovery detection
# --------------------------------------------------------
info "--- Test 5: Recovery Detection ---"
info "Restoring healthy.txt on target-service..."
compose exec -T target-service sh -c 'echo OK > /usr/share/nginx/html/healthy.txt'

# Count current healthy notifications before waiting for a new one
HEALTHY_COUNT_BEFORE=$(compose logs proxy-simulator 2>/dev/null | grep "RECV:" | grep "notification_batch" | grep -c '"state":"healthy"' || true)

info "Waiting up to 45s for recovery notification..."

GOT_RECOVERY=false
for i in $(seq 1 45); do
    HEALTHY_COUNT=$(compose logs proxy-simulator 2>/dev/null | grep "RECV:" | grep "notification_batch" | grep -c '"state":"healthy"' || true)
    if [ "$HEALTHY_COUNT" -gt "$HEALTHY_COUNT_BEFORE" ]; then
        GOT_RECOVERY=true
        break
    fi
    sleep 1
done

if $GOT_RECOVERY; then
    pass "HTTP check recovered to healthy"
else
    fail "No recovery notification within 45s"
fi

# --------------------------------------------------------
# Summary
# --------------------------------------------------------
echo ""
echo "========================================"
echo "  E2E Test Results: $PASS/$TOTAL passed"
echo "========================================"

if [ $FAIL -gt 0 ]; then
    echo ""
    info "=== Full proxy-simulator logs ==="
    compose logs proxy-simulator 2>/dev/null || true
    echo ""
    info "=== Full healthcheck-server logs ==="
    compose logs healthcheck-server 2>/dev/null || true
    echo ""
    echo -e "${RED}FAILED ($FAIL failures)${NC}"
    exit 1
else
    echo -e "${GREEN}ALL TESTS PASSED${NC}"
    exit 0
fi
