#!/bin/bash
# Integration test for hybrid Rust+Go healthcheck architecture
# Tests communication between Rust server and Go proxy (simulated)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
SERVER_BIN="$PROJECT_ROOT/rust/target/debug/healthcheck-server"
SERVER_DIR="$(dirname "$SERVER_BIN")"
SOCKET_PATH="/tmp/healthcheck-hybrid-test.sock"
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
    log_info "Cleaning up..."
    if [ -n "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi
    rm -f "$SOCKET_PATH" "$CONFIG_FILE"
}

trap cleanup EXIT INT TERM

# Create config
create_config() {
    log_info "Creating server config"
    cat > "$CONFIG_FILE" <<EOF
server:
  proxy_socket: "$SOCKET_PATH"

batching:
  delay: 100ms
  max_size: 10

manager:
  monitor_interval: 500ms

metrics:
  enabled: false

telemetry:
  enabled: false

logging:
  level: "info"
  format: "text"
EOF
}

# Start Rust server
start_server() {
    log_info "Starting Rust healthcheck server..."
    rm -f "$SOCKET_PATH"
    cd "$SERVER_DIR"
    "$SERVER_BIN" > /tmp/hybrid-test-server.log 2>&1 &
    SERVER_PID=$!
    cd - > /dev/null

    # Wait for socket
    for i in {1..50}; do
        if [ -S "$SOCKET_PATH" ]; then
            log_info "Server started OK (PID: $SERVER_PID)"
            return 0
        fi
        sleep 0.1
    done

    log_error "Server failed to start"
    cat /tmp/hybrid-test-server.log
    return 1
}

# Run all tests over a single persistent connection.
# The server proxy only accepts one connection, so all tests
# must share the same session.
run_all_tests() {
    local failed_tests=""
    local num_failed=0

    # Use bash coproc for bidirectional socat communication
    coproc SOCAT { socat - UNIX-CONNECT:"$SOCKET_PATH"; }
    local SOCAT_PID=$SOCAT_PID

    # Give socat a moment to connect
    sleep 0.5

    # ━━━ Test 1: Socket Connection ━━━
    log_test "━━━ Test 1: Socket Connection ━━━"
    if [ -S "$SOCKET_PATH" ] && kill -0 $SOCAT_PID 2>/dev/null; then
        log_info "Socket connection OK"
    else
        log_error "Cannot connect to socket"
        failed_tests="${failed_tests}\n  - Socket Connection"
        num_failed=$((num_failed + 1))
    fi

    # ━━━ Test 2: Ready Message ━━━
    log_test "━━━ Test 2: Ready Message ━━━"
    local ready_msg=""
    # Read from socat stdout with a timeout
    read -t 2 ready_msg <&${SOCAT[0]} || true

    if [ -z "$ready_msg" ]; then
        log_error "No message received from server"
        failed_tests="${failed_tests}\n  - Ready Message"
        num_failed=$((num_failed + 1))
    else
        local msg_type=$(echo "$ready_msg" | jq -r '.type' 2>/dev/null || echo "")
        if [ "$msg_type" = "ready" ]; then
            log_info "Ready message received: $ready_msg"
        else
            log_error "Expected 'ready' message, got: $ready_msg"
            failed_tests="${failed_tests}\n  - Ready Message"
            num_failed=$((num_failed + 1))
        fi
    fi

    # ━━━ Test 3: Send Config Update ━━━
    log_test "━━━ Test 3: Send Config Update ━━━"
    local config_msg='{"type":"update_configs","configs":[{"id":1,"interval":"5s","timeout":"2s","retries":2,"checker_type":"tcp","ip":"127.0.0.1","port":22}]}'

    # Send config via socat stdin
    echo "$config_msg" >&${SOCAT[1]}
    sleep 2

    if grep -q "Received.*healthcheck configs\|Adding healthcheck\|Received message from Go proxy" /tmp/hybrid-test-server.log 2>/dev/null; then
        log_info "Config update processed OK"
    else
        log_error "Config update not processed"
        cat /tmp/hybrid-test-server.log
        failed_tests="${failed_tests}\n  - Send Config"
        num_failed=$((num_failed + 1))
    fi

    # ━━━ Test 4: Message Format Verification ━━━
    log_test "━━━ Test 4: Message Format Verification ━━━"
    # Verify server logs show no parse errors for our config message
    if ! grep -q "Failed to parse proxy message.*update_configs" /tmp/hybrid-test-server.log 2>/dev/null; then
        log_info "Message format OK (no parse errors)"
    else
        log_error "Message format error detected"
        failed_tests="${failed_tests}\n  - Message Format"
        num_failed=$((num_failed + 1))
    fi

    # Close the connection
    exec {SOCAT[1]}>&-
    wait $SOCAT_PID 2>/dev/null || true

    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    if [ $num_failed -eq 0 ]; then
        log_info "OK All Hybrid Integration Tests PASSED!"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        echo ""
        log_info "Rust server is ready for Go proxy integration!"
        return 0
    else
        log_error "X $num_failed Tests FAILED:"
        echo -e "$failed_tests"
        echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
        return 1
    fi
}

# Main test execution
main() {
    log_info "Starting Hybrid Go-Rust Integration Test"
    log_info "Project root: $PROJECT_ROOT"
    echo ""

    # Check for required tools
    if ! command -v socat &>/dev/null; then
        log_error "socat is required but not installed. Install with: apt install socat"
        exit 1
    fi
    if ! command -v jq &>/dev/null; then
        log_error "jq is required but not installed. Install with: apt install jq"
        exit 1
    fi

    # Build if needed
    if [ ! -f "$SERVER_BIN" ]; then
        log_info "Building server..."
        cd "$PROJECT_ROOT/rust"
        cargo build -p healthcheck-server
    fi

    # Create config and start server
    create_config
    start_server || exit 1

    run_all_tests
}

main "$@"
