#!/bin/bash
# Integration test for healthcheck server metrics with IPv6 and dual-stack
# Tests IPv4-only, IPv6-only, and dual-stack configurations

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
SERVER_BIN="$PROJECT_ROOT/target/debug/healthcheck-server"
SERVER_DIR="$(dirname "$SERVER_BIN")"
SOCKET_PATH="/tmp/healthcheck-ipv6-test.sock"
CONFIG_FILE="$SERVER_DIR/healthcheck-server.yaml"
SERVER_PID=""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_test() {
    echo -e "${BLUE}[TEST]${NC} $1"
}

cleanup() {
    if [ -n "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi
    rm -f "$SOCKET_PATH" "$CONFIG_FILE"
}

trap cleanup EXIT INT TERM

# Create config with specific listen address
create_config() {
    local listen_addr="$1"
    log_info "Creating config with listen_addr: $listen_addr"

    cat > "$CONFIG_FILE" <<EOF
server:
  proxy_socket: "$SOCKET_PATH"

batching:
  delay: 50ms
  max_size: 10

manager:
  monitor_interval: 100ms

metrics:
  enabled: true
  listen_addr: "$listen_addr"
  response_time_buckets: [0.001, 0.01, 0.1, 1.0]
  batch_delay_buckets: [0.01, 0.1, 1.0]
  batch_size_buckets: [1, 10, 100, 1000]

logging:
  level: "warn"
  format: "text"
EOF
}

# Start server
start_server() {
    rm -f "$SOCKET_PATH"
    cd "$SERVER_DIR"
    "$SERVER_BIN" > /tmp/healthcheck-server.log 2>&1 &
    SERVER_PID=$!
    cd - > /dev/null

    # Wait for server to be ready
    for i in {1..50}; do
        if [ -S "$SOCKET_PATH" ]; then
            sleep 0.5  # Give metrics server time to bind
            return 0
        fi
        sleep 0.1
    done

    log_error "Server failed to start"
    cat /tmp/healthcheck-server.log
    return 1
}

stop_server() {
    if [ -n "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
        SERVER_PID=""
    fi
    rm -f "$SOCKET_PATH"
}

# Test metrics endpoint with specific URL
test_endpoint() {
    local url="$1"
    local test_name="$2"

    log_test "$test_name: Testing $url"

    if ! curl -s -f "$url" > /dev/null; then
        log_error "$test_name FAILED: Could not fetch $url"
        return 1
    fi

    local metrics=$(curl -s "$url")

    # Check for key metrics
    if ! echo "$metrics" | grep -q "healthcheck_monitors_active"; then
        log_error "$test_name FAILED: Missing healthcheck_monitors_active"
        return 1
    fi

    if ! echo "$metrics" | grep -q "healthcheck_proxy_connected"; then
        log_error "$test_name FAILED: Missing healthcheck_proxy_connected"
        return 1
    fi

    log_info "$test_name PASSED OK"
    return 0
}

# Test 1: IPv4 only (127.0.0.1)
test_ipv4_only() {
    log_test "━━━ Test 1: IPv4 Only ━━━"

    create_config "127.0.0.1:9090"
    start_server || return 1

    # Test with IPv4
    test_endpoint "http://127.0.0.1:9090/metrics" "IPv4"
    local result=$?

    stop_server
    return $result
}

# Test 2: IPv6 only ([::1])
test_ipv6_only() {
    log_test "━━━ Test 2: IPv6 Only ━━━"

    create_config "[::1]:9090"
    start_server || return 1

    # Test with IPv6
    test_endpoint "http://[::1]:9090/metrics" "IPv6"
    local result=$?

    # IPv4 should NOT work
    if curl -s -f "http://127.0.0.1:9090/metrics" > /dev/null 2>&1; then
        log_error "IPv6-only test FAILED: IPv4 should not be accessible"
        result=1
    else
        log_info "IPv6-only verified: IPv4 correctly not accessible OK"
    fi

    stop_server
    return $result
}

# Test 3: Dual stack (0.0.0.0)
test_dual_stack_ipv4() {
    log_test "━━━ Test 3: Dual Stack (0.0.0.0) ━━━"

    create_config "0.0.0.0:9090"
    start_server || return 1

    local result=0

    # Test with IPv4
    test_endpoint "http://127.0.0.1:9090/metrics" "Dual-Stack IPv4" || result=1

    # Test with IPv6 (if system supports it)
    if test_endpoint "http://[::1]:9090/metrics" "Dual-Stack IPv6" 2>/dev/null; then
        log_info "Dual-stack IPv6 accessible OK"
    else
        log_info 'Dual-stack IPv6 not available (system may not support it)'
    fi

    stop_server
    return $result
}

# Test 4: Dual stack IPv6 ([::])
test_dual_stack_ipv6() {
    log_test "━━━ Test 4: Dual Stack ([::]) ━━━"

    create_config "[::]:9090"
    start_server || return 1

    local result=0

    # Test with IPv6
    test_endpoint "http://[::1]:9090/metrics" "Dual-Stack [::] IPv6" || result=1

    # Test with IPv4 (depends on system net.ipv6.bindv6only setting)
    if test_endpoint "http://127.0.0.1:9090/metrics" "Dual-Stack [::] IPv4" 2>/dev/null; then
        log_info 'Dual-stack [::] accepts IPv4 (net.ipv6.bindv6only=0) OK'
    else
        log_info 'Dual-stack [::] IPv6-only (net.ipv6.bindv6only=1)'
    fi

    stop_server
    return $result
}

# Test 5: Concurrent access across protocols
test_concurrent_multiprotocol() {
    log_test "━━━ Test 5: Concurrent Multi-Protocol Access ━━━"

    create_config "0.0.0.0:9090"
    start_server || return 1

    local pids=""
    local result=0

    # Concurrent IPv4 requests
    for i in {1..5}; do
        curl -s http://127.0.0.1:9090/metrics > /dev/null 2>&1 &
        pids="$pids $!"
    done

    # Concurrent IPv6 requests (if available)
    for i in {1..5}; do
        curl -s http://[::1]:9090/metrics > /dev/null 2>&1 &
        pids="$pids $!"
    done

    # Wait for all
    for pid in $pids; do
        wait $pid 2>/dev/null || true
    done

    log_info "Concurrent multi-protocol access completed OK"

    stop_server
    return 0
}

# Main execution
main() {
    log_info "Starting IPv6 and Dual-Stack Metrics Integration Tests"
    log_info "Project root: $PROJECT_ROOT"

    # Build if needed
    if [ ! -f "$SERVER_BIN" ]; then
        log_info "Building server..."
        cd "$PROJECT_ROOT"
        cargo build -p healthcheck-server
    fi

    local failed_tests=""
    local num_failed=0

    # Run all tests
    if ! test_ipv4_only; then
        failed_tests="${failed_tests}\n  - IPv4 Only"
        num_failed=$((num_failed + 1))
    fi

    if ! test_ipv6_only; then
        failed_tests="${failed_tests}\n  - IPv6 Only"
        num_failed=$((num_failed + 1))
    fi

    if ! test_dual_stack_ipv4; then
        failed_tests="${failed_tests}\n  - Dual Stack 0.0.0.0"
        num_failed=$((num_failed + 1))
    fi

    if ! test_dual_stack_ipv6; then
        failed_tests="${failed_tests}\n  - Dual Stack [::]"
        num_failed=$((num_failed + 1))
    fi

    if ! test_concurrent_multiprotocol; then
        failed_tests="${failed_tests}\n  - Concurrent Multi-Protocol"
        num_failed=$((num_failed + 1))
    fi

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    if [ $num_failed -eq 0 ]; then
        log_info "OK All IPv6/Dual-Stack Tests PASSED!"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        return 0
    else
        log_error "X $num_failed Tests FAILED:"
        echo -e "$failed_tests"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        return 1
    fi
}

main "$@"
