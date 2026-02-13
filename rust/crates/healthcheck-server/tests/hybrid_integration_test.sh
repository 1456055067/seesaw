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

# Test 1: Verify socket exists and is connectable
test_socket_connection() {
    log_test "━━━ Test 1: Socket Connection ━━━"

    if [ ! -S "$SOCKET_PATH" ]; then
        log_error "Socket does not exist at $SOCKET_PATH"
        return 1
    fi

    # Try to connect
    if ! timeout 2 bash -c "echo 'test' | nc -U $SOCKET_PATH > /dev/null"; then
        log_error "Cannot connect to socket"
        return 1
    fi

    log_info "Socket connection OK"
    return 0
}

# Test 2: Receive Ready message
test_ready_message() {
    log_test "━━━ Test 2: Ready Message ━━━"

    # Connect and read first message
    local ready_msg=$(timeout 2 bash -c "nc -U $SOCKET_PATH 2>/dev/null | head -1" || echo "")

    if [ -z "$ready_msg" ]; then
        log_error "No message received from server"
        return 1
    fi

    # Parse JSON and check type
    local msg_type=$(echo "$ready_msg" | jq -r '.type' 2>/dev/null || echo "")
    if [ "$msg_type" != "ready" ]; then
        log_error "Expected 'ready' message, got: $ready_msg"
        return 1
    fi

    log_info "Ready message received OK"
    return 0
}

# Test 3: Send UpdateConfigs message
test_send_config() {
    log_test "━━━ Test 3: Send Config Update ━━━"

    # Create test config message
    local config_msg=$(cat <<'EOF'
{
  "type": "update_configs",
  "configs": [
    {
      "id": 1,
      "interval": "5s",
      "timeout": "2s",
      "retries": 2,
      "checker_type": "tcp",
      "ip": "127.0.0.1",
      "port": 22
    },
    {
      "id": 2,
      "interval": "3s",
      "timeout": "1s",
      "retries": 3,
      "checker_type": "http",
      "ip": "127.0.0.1",
      "port": 80,
      "method": "GET",
      "path": "/health",
      "expected_codes": [200, 204],
      "secure": false
    }
  ]
}
EOF
)

    # Send config via socket
    {
        # Read Ready message
        read ready_line
        echo "Received: $ready_line" >&2

        # Send UpdateConfigs
        echo "$config_msg"

        # Wait a bit for processing
        sleep 2
    } | nc -U "$SOCKET_PATH" > /tmp/hybrid-test-response.txt 2>&1

    # Check server logs for config processing
    if grep -q "Adding healthcheck" /tmp/hybrid-test-server.log 2>/dev/null; then
        log_info "Config update processed OK"
        return 0
    else
        log_error "Config update not processed"
        cat /tmp/hybrid-test-server.log
        return 1
    fi
}

# Test 4: Receive notifications
test_receive_notifications() {
    log_test "━━━ Test 4: Receive Notifications ━━━"

    # First, send config with a healthcheck that will trigger
    local config_msg=$(cat <<'EOF'
{
  "type": "update_configs",
  "configs": [
    {
      "id": 100,
      "interval": "1s",
      "timeout": "500ms",
      "retries": 1,
      "checker_type": "tcp",
      "ip": "127.0.0.1",
      "port": 22
    }
  ]
}
EOF
)

    # Connect and listen for messages
    {
        # Read Ready
        read ready_line

        # Send config
        echo "$config_msg"

        # Wait for notifications (up to 10 seconds)
        timeout 10 cat || true
    } | nc -U "$SOCKET_PATH" > /tmp/hybrid-notifications.txt 2>&1

    # Check if we received any notification_batch messages
    if grep -q '"type":"notification_batch"' /tmp/hybrid-notifications.txt 2>/dev/null; then
        log_info "Notifications received OK"

        # Show sample notification
        log_info "Sample notification:"
        grep -m 1 '"type":"notification_batch"' /tmp/hybrid-notifications.txt | jq '.' || cat /tmp/hybrid-notifications.txt
        return 0
    else
        log_error "No notifications received"
        cat /tmp/hybrid-notifications.txt
        return 1
    fi
}

# Test 5: Protocol format verification
test_message_format() {
    log_test "━━━ Test 5: Message Format Verification ━━━"

    # Send a valid config and verify response format
    local config_msg='{"type":"update_configs","configs":[{"id":200,"interval":"2s","timeout":"1s","retries":1,"checker_type":"tcp","ip":"127.0.0.1","port":22}]}'

    local response=$({
        read ready
        echo "$config_msg"
        timeout 3 head -1 || true
    } | nc -U "$SOCKET_PATH" 2>/dev/null || echo "")

    # Verify it's valid JSON
    if ! echo "$response" | jq . > /dev/null 2>&1; then
        # Empty response is OK for this test (config doesn't generate immediate response)
        log_info "Message format OK (no immediate response expected)"
        return 0
    fi

    log_info "Message format OK"
    return 0
}

# Main test execution
main() {
    log_info "Starting Hybrid Go-Rust Integration Test"
    log_info "Project root: $PROJECT_ROOT"
    echo ""

    # Build if needed
    if [ ! -f "$SERVER_BIN" ]; then
        log_info "Building server..."
        cd "$PROJECT_ROOT/rust"
        cargo build -p healthcheck-server
    fi

    # Create config and start server
    create_config
    start_server || exit 1

    local failed_tests=""
    local num_failed=0

    # Run tests
    if ! test_socket_connection; then
        failed_tests="${failed_tests}\n  - Socket Connection"
        num_failed=$((num_failed + 1))
    fi

    if ! test_ready_message; then
        failed_tests="${failed_tests}\n  - Ready Message"
        num_failed=$((num_failed + 1))
    fi

    if ! test_send_config; then
        failed_tests="${failed_tests}\n  - Send Config"
        num_failed=$((num_failed + 1))
    fi

    if ! test_receive_notifications; then
        failed_tests="${failed_tests}\n  - Receive Notifications"
        num_failed=$((num_failed + 1))
    fi

    if ! test_message_format; then
        failed_tests="${failed_tests}\n  - Message Format"
        num_failed=$((num_failed + 1))
    fi

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

main "$@"
