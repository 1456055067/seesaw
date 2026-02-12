#!/bin/bash
# Integration test for Prometheus + Grafana monitoring stack
# Tests that the monitoring stack can scrape healthcheck-server metrics

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
HEALTHCHECK_DIR="$PROJECT_ROOT/crates/healthcheck-server"
SERVER_BIN="$PROJECT_ROOT/target/debug/healthcheck-server"
SERVER_DIR="$(dirname "$SERVER_BIN")"
SOCKET_PATH="/tmp/healthcheck-monitoring-test.sock"
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

    # Stop healthcheck server
    if [ -n "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi
    rm -f "$SOCKET_PATH" "$CONFIG_FILE"

    # Stop docker-compose stack
    cd "$HEALTHCHECK_DIR"
    docker-compose down -v 2>/dev/null || true

    log_info "Cleanup complete"
}

trap cleanup EXIT INT TERM

# Create healthcheck server config
create_healthcheck_config() {
    log_info "Creating healthcheck server config"
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
  listen_addr: "127.0.0.1:9090"
  response_time_buckets: [0.001, 0.01, 0.1, 1.0]
  batch_delay_buckets: [0.01, 0.1, 1.0]
  batch_size_buckets: [1, 10, 100, 1000]

logging:
  level: "warn"
  format: "text"
EOF
}

# Start healthcheck server
start_healthcheck_server() {
    log_info "Starting healthcheck server..."
    rm -f "$SOCKET_PATH"
    cd "$SERVER_DIR"
    "$SERVER_BIN" > /tmp/healthcheck-monitoring-test.log 2>&1 &
    SERVER_PID=$!
    cd - > /dev/null

    # Wait for server to be ready
    for i in {1..50}; do
        if [ -S "$SOCKET_PATH" ]; then
            sleep 1  # Give metrics server time to bind
            log_info "Healthcheck server started OK (PID: $SERVER_PID)"
            return 0
        fi
        sleep 0.1
    done

    log_error "Healthcheck server failed to start"
    cat /tmp/healthcheck-monitoring-test.log
    return 1
}

# Verify healthcheck metrics endpoint
test_healthcheck_metrics() {
    log_test "Verifying healthcheck metrics endpoint"

    if ! curl -s -f http://127.0.0.1:9090/metrics > /dev/null; then
        log_error "Healthcheck metrics endpoint not accessible"
        return 1
    fi

    local metrics=$(curl -s http://127.0.0.1:9090/metrics)

    if ! echo "$metrics" | grep -q "healthcheck_monitors_active"; then
        log_error "Healthcheck metrics missing expected data"
        return 1
    fi

    log_info "Healthcheck metrics endpoint OK"
    return 0
}

# Start monitoring stack
start_monitoring_stack() {
    log_info "Starting Prometheus + Grafana stack..."
    cd "$HEALTHCHECK_DIR"

    # Pull images first
    docker-compose pull

    # Start services
    docker-compose up -d

    log_info "Waiting for services to start..."
    sleep 5

    # Check services are running
    if ! docker-compose ps | grep -q "healthcheck-prometheus.*Up"; then
        log_error "Prometheus container not running"
        docker-compose logs prometheus
        return 1
    fi

    if ! docker-compose ps | grep -q "healthcheck-grafana.*Up"; then
        log_error "Grafana container not running"
        docker-compose logs grafana
        return 1
    fi

    log_info "Monitoring stack started OK"
    return 0
}

# Test Prometheus UI
test_prometheus_ui() {
    log_test "Testing Prometheus UI"

    # Wait for Prometheus to be ready
    for i in {1..30}; do
        if curl -s http://localhost:9091/-/ready > /dev/null 2>&1; then
            log_info "Prometheus UI accessible OK"
            return 0
        fi
        sleep 1
    done

    log_error "Prometheus UI not accessible"
    return 1
}

# Test Prometheus scraping healthcheck-server
test_prometheus_scraping() {
    log_test "Testing Prometheus scraping healthcheck-server"

    # Wait for at least one scrape
    log_info "Waiting for Prometheus to scrape healthcheck-server..."
    sleep 20

    # Query Prometheus API for healthcheck metrics
    local query="healthcheck_monitors_active"
    local response=$(curl -s "http://localhost:9091/api/v1/query?query=$query")

    if ! echo "$response" | grep -q '"status":"success"'; then
        log_error "Prometheus query failed"
        echo "$response"
        return 1
    fi

    if echo "$response" | grep -q '"result":\[\]'; then
        log_error "Prometheus has no data for healthcheck_monitors_active"
        log_error "Checking Prometheus targets..."
        curl -s http://localhost:9091/api/v1/targets | jq '.'
        return 1
    fi

    log_info "Prometheus scraping healthcheck-server OK"
    return 0
}

# Test Grafana UI
test_grafana_ui() {
    log_test "Testing Grafana UI"

    # Wait for Grafana to be ready
    for i in {1..60}; do
        if curl -s http://localhost:3000/api/health > /dev/null 2>&1; then
            log_info "Grafana UI accessible OK"
            return 0
        fi
        sleep 1
    done

    log_error "Grafana UI not accessible"
    return 1
}

# Test Grafana data source
test_grafana_datasource() {
    log_test "Testing Grafana Prometheus data source"

    # Login and get data sources
    local response=$(curl -s -u admin:admin http://localhost:3000/api/datasources)

    if ! echo "$response" | grep -q '"name":"Prometheus"'; then
        log_error "Prometheus data source not configured in Grafana"
        echo "$response"
        return 1
    fi

    if ! echo "$response" | grep -q '"type":"prometheus"'; then
        log_error "Data source is not of type prometheus"
        return 1
    fi

    log_info "Grafana Prometheus data source configured OK"
    return 0
}

# Test Grafana dashboard provisioned
test_grafana_dashboard() {
    log_test "Testing Grafana dashboard provisioning"

    # Get dashboards
    local response=$(curl -s -u admin:admin http://localhost:3000/api/search?type=dash-db)

    if ! echo "$response" | grep -q '"title":"Healthcheck Server Metrics"'; then
        log_error "Healthcheck Server Metrics dashboard not found"
        echo "$response"
        return 1
    fi

    log_info "Grafana dashboard provisioned OK"
    return 0
}

# Show monitoring stack status
show_status() {
    log_info "Monitoring Stack Status:"
    echo ""
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Healthcheck Server:"
    echo "  - Metrics endpoint: http://localhost:9090/metrics"
    echo "  - Status: $(curl -s -o /dev/null -w "%{http_code}" http://localhost:9090/metrics)"
    echo ""
    echo "Prometheus:"
    echo "  - UI: http://localhost:9091"
    echo "  - Status: $(curl -s http://localhost:9091/-/ready 2>/dev/null && echo "Ready" || echo "Not Ready")"
    echo ""
    echo "Grafana:"
    echo "  - UI: http://localhost:3000"
    echo "  - Credentials: admin / admin"
    echo "  - Status: $(curl -s http://localhost:3000/api/health 2>/dev/null | grep -q "ok" && echo "Ready" || echo "Not Ready")"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
}

# Main execution
main() {
    log_info "Starting Prometheus + Grafana Monitoring Stack Integration Test"
    log_info "Project root: $PROJECT_ROOT"

    # Build healthcheck server if needed
    if [ ! -f "$SERVER_BIN" ]; then
        log_info "Building healthcheck server..."
        cd "$PROJECT_ROOT"
        cargo build -p healthcheck-server
    fi

    # Create config and start healthcheck server
    create_healthcheck_config
    start_healthcheck_server || exit 1
    test_healthcheck_metrics || exit 1

    # Start and test monitoring stack
    start_monitoring_stack || exit 1
    test_prometheus_ui || exit 1
    test_prometheus_scraping || exit 1
    test_grafana_ui || exit 1
    test_grafana_datasource || exit 1
    test_grafana_dashboard || exit 1

    # Show final status
    show_status

    echo ""
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "OK All Monitoring Stack Tests PASSED!"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    log_info "You can now access:"
    log_info "  - Prometheus: http://localhost:9091"
    log_info "  - Grafana: http://localhost:3000 (admin/admin)"
    echo ""
    log_info "Press Ctrl+C to stop and cleanup..."

    # Keep running until user stops
    wait

    return 0
}

main "$@"
