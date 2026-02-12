#!/bin/bash
# Integration test for healthcheck server with metrics
# Tests the full Rust server with metrics endpoint

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
SERVER_BIN="$PROJECT_ROOT/target/debug/healthcheck-server"
SERVER_DIR="$(dirname "$SERVER_BIN")"
SOCKET_PATH="/tmp/healthcheck-integration-test.sock"
CONFIG_FILE="$SERVER_DIR/healthcheck-server.yaml"
SERVER_PID=""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

cleanup() {
    log_info "Cleaning up..."
    if [ -n "$SERVER_PID" ]; then
        log_info "Stopping server (PID: $SERVER_PID)"
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi
    rm -f "$SOCKET_PATH" "$CONFIG_FILE"
    log_info "Cleanup complete"
}

trap cleanup EXIT INT TERM

# Create test configuration
create_config() {
    log_info "Creating test configuration at $CONFIG_FILE"
    cat > "$CONFIG_FILE" <<EOF
server:
  proxy_socket: "$SOCKET_PATH"

batching:
  delay: 50ms
  max_size: 10

channels:
  notification: 100
  config_update: 10
  proxy_message: 10

manager:
  monitor_interval: 100ms

metrics:
  enabled: true
  listen_addr: "127.0.0.1:9090"
  response_time_buckets: [0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
  batch_delay_buckets: [0.01, 0.025, 0.05, 0.075, 0.1, 0.15, 0.2, 0.25, 0.5, 1.0]
  batch_size_buckets: [1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000]

logging:
  level: "info"
  format: "text"
EOF
}

# Build server if needed
build_server() {
    if [ ! -f "$SERVER_BIN" ]; then
        log_info "Building healthcheck-server..."
        cd "$PROJECT_ROOT"
        cargo build -p healthcheck-server
    else
        log_info "Using existing binary at $SERVER_BIN"
    fi
}

# Start server
start_server() {
    log_info "Starting healthcheck server with metrics enabled..."
    log_info "Config file: $CONFIG_FILE"
    rm -f "$SOCKET_PATH"

    # Start server in background (run from SERVER_DIR so it finds ./healthcheck-server.yaml)
    cd "$SERVER_DIR"
    "$SERVER_BIN" &
    SERVER_PID=$!
    cd - > /dev/null

    log_info "Server started (PID: $SERVER_PID)"

    # Wait for server to be ready
    log_info "Waiting for server to start..."
    for i in {1..30}; do
        if [ -S "$SOCKET_PATH" ]; then
            log_info "Server socket ready at $SOCKET_PATH"
            return 0
        fi
        sleep 0.1
    done

    log_error "Server failed to start (socket not found)"
    return 1
}

# Test metrics endpoint
test_metrics_endpoint() {
    log_info "Testing metrics endpoint..."

    # Wait for metrics server to start
    sleep 1

    # Fetch metrics
    log_info "Fetching metrics from http://127.0.0.1:9090/metrics"
    if ! METRICS=$(curl -s http://127.0.0.1:9090/metrics); then
        log_error "Failed to fetch metrics endpoint"
        return 1
    fi

    log_info "Metrics endpoint accessible!"

    # Check for expected metrics
    log_info "Verifying metric families..."

    local expected_metrics=(
        "healthcheck_monitors_active"
        "healthcheck_proxy_connected"
        "healthcheck_checks_total"
        "healthcheck_response_time_seconds"
        "healthcheck_state"
        "healthcheck_consecutive_successes"
        "healthcheck_consecutive_failures"
        "healthcheck_state_transitions_total"
        "healthcheck_notifications_batched_total"
        "healthcheck_notifications_sent_total"
        "healthcheck_batch_size"
        "healthcheck_batch_delay_seconds"
        "healthcheck_config_updates_total"
        "healthcheck_errors_total"
        "healthcheck_monitor_task_duration_seconds"
    )

    local missing_metrics=()
    for metric in "${expected_metrics[@]}"; do
        if ! echo "$METRICS" | grep -q "^# TYPE $metric"; then
            missing_metrics+=("$metric")
        else
            log_info "  ✓ Found metric: $metric"
        fi
    done

    if [ ${#missing_metrics[@]} -gt 0 ]; then
        log_error "Missing metrics:"
        for metric in "${missing_metrics[@]}"; do
            log_error "  ✗ $metric"
        done
        return 1
    fi

    log_info "All expected metric families present!"

    # Check initial values
    log_info "Checking initial metric values..."

    # Should have 0 active monitors (no healthchecks configured yet)
    if echo "$METRICS" | grep -q "healthcheck_monitors_active 0"; then
        log_info "  ✓ Active monitors: 0 (expected)"
    else
        log_warn "  ! Active monitors not 0 (may have been started already)"
    fi

    # Proxy should be disconnected (0)
    if echo "$METRICS" | grep -q "healthcheck_proxy_connected 0"; then
        log_info "  ✓ Proxy connected: 0 (expected, no proxy running)"
    else
        log_warn "  ! Proxy connection status unexpected"
    fi

    return 0
}

# Test metrics content type
test_metrics_headers() {
    log_info "Testing metrics HTTP headers..."

    HEADERS=$(curl -s -I http://127.0.0.1:9090/metrics)

    if echo "$HEADERS" | grep -q "content-type: text/plain; version=0.0.4"; then
        log_info "  ✓ Correct content-type header"
    else
        log_error "  ✗ Incorrect content-type header"
        echo "$HEADERS"
        return 1
    fi

    if echo "$HEADERS" | grep -q "HTTP/1.1 200 OK"; then
        log_info "  ✓ HTTP 200 OK status"
    else
        log_error "  ✗ Incorrect HTTP status"
        return 1
    fi

    return 0
}

# Test concurrent metric access
test_concurrent_access() {
    log_info "Testing concurrent metric access..."

    local pids=()
    for i in {1..10}; do
        curl -s http://127.0.0.1:9090/metrics > /dev/null &
        pids+=($!)
    done

    # Wait for all requests
    for pid in "${pids[@]}"; do
        if ! wait $pid; then
            log_error "Concurrent request failed"
            return 1
        fi
    done

    log_info "  ✓ All 10 concurrent requests succeeded"
    return 0
}

# Display sample metrics
show_sample_metrics() {
    log_info "Sample metrics output:"
    echo "----------------------------------------"
    curl -s http://127.0.0.1:9090/metrics | head -50
    echo "----------------------------------------"
}

# Main test execution
main() {
    log_info "Starting healthcheck server integration test"
    log_info "Project root: $PROJECT_ROOT"

    create_config
    build_server
    start_server

    log_info "Running metrics tests..."

    if ! test_metrics_endpoint; then
        log_error "Metrics endpoint test failed"
        exit 1
    fi

    if ! test_metrics_headers; then
        log_error "Metrics headers test failed"
        exit 1
    fi

    if ! test_concurrent_access; then
        log_error "Concurrent access test failed"
        exit 1
    fi

    show_sample_metrics

    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "✓ All integration tests PASSED!"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    return 0
}

main "$@"
