# Hybrid Healthcheck Server Deployment Guide

This guide explains how to build, test, and deploy the hybrid Rust+Go healthcheck server for Seesaw.

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Prerequisites](#prerequisites)
- [Building](#building)
- [Testing](#testing)
- [Deployment](#deployment)
  - [Systemd Service Files](#systemd-service-files)
  - [Installation Steps](#installation-steps)
  - [Configuration (Optional)](#configuration-optional)
- [Monitoring](#monitoring)
- [Troubleshooting](#troubleshooting)
- [Performance Validation](#performance-validation)

## Architecture Overview

The hybrid architecture consists of two components:

1. **Rust Healthcheck Server** - High-performance health checking engine
   - Location: `rust/crates/healthcheck-server`
   - Binary: `healthcheck-server`
   - Socket: `/var/run/seesaw/healthcheck-proxy.sock` (configurable)

2. **Go RPC Proxy** - Thin bridge to Seesaw Engine
   - Location: `healthcheck/server/main.go`
   - Binary: `healthcheck-proxy`
   - Connects to: Engine RPC + Rust server socket

### Message Flow

```
Engine (RPC) ←→ Go Proxy (JSON/Socket) ←→ Rust Server (Monitors)
```

## Prerequisites

### For Building

- **Rust**: 1.70+ (`rustc --version`)
- **Go**: 1.19+ (`go version`)
- **cargo**: Latest stable
- **Dependencies**: See `rust/Cargo.toml` and `go.mod`

### For Deployment

- **OS**: Linux (tested on Ubuntu 20.04+, Debian 11+)
- **Seesaw Engine**: Must be running for full integration
- **Socket directory**: `/var/run/seesaw/` (create if needed)
- **Permissions**: Write access to `/var/run/seesaw/`

## Building

### Rust Server

```bash
cd rust

# Development build (with debug symbols)
cargo build -p healthcheck-server

# Release build (optimized, for production)
cargo build --release -p healthcheck-server

# Output location:
# - Debug: rust/target/debug/healthcheck-server
# - Release: rust/target/release/healthcheck-server
```

### Go Proxy

```bash
# From repository root
go build -o bin/healthcheck-proxy ./healthcheck/server/main.go

# With optimizations
go build -ldflags="-s -w" -o bin/healthcheck-proxy ./healthcheck/server/main.go

# Output: bin/healthcheck-proxy
```

### Build Both

```bash
# Convenience script (create this)
#!/bin/bash
set -e

echo "Building Rust server..."
cd rust && cargo build --release -p healthcheck-server
cd ..

echo "Building Go proxy..."
go build -ldflags="-s -w" -o bin/healthcheck-proxy ./healthcheck/server/main.go

echo "Build complete:"
echo "  Rust: rust/target/release/healthcheck-server"
echo "  Go:   bin/healthcheck-proxy"
```

## Testing

### Unit and Integration Tests

```bash
# Run all Rust tests
cd rust
cargo test -p healthcheck-server

# Run specific test suites
cargo test -p healthcheck-server --test manager_test
cargo test -p healthcheck-server --test notifier_test
cargo test -p healthcheck-server --test proxy_test

# With output
cargo test -p healthcheck-server -- --nocapture

# Run Go tests
cd ..
go test ./healthcheck/server/...
```

### Manual Integration Test

#### Terminal 1: Start Rust Server

```bash
# Create socket directory
sudo mkdir -p /var/run/seesaw
sudo chown $(whoami) /var/run/seesaw

# Run with logging
RUST_LOG=debug ./rust/target/debug/healthcheck-server
```

**Expected output:**
```
INFO healthcheck_server: Starting healthcheck server
INFO Proxy listener started socket=/var/run/seesaw/healthcheck-proxy.sock
INFO Go proxy connected
INFO Sent Ready message to Go proxy
INFO All tasks spawned, server running
```

#### Terminal 2: Start Go Proxy (requires Engine)

```bash
# With default Engine socket
./bin/healthcheck-proxy

# With custom sockets
./bin/healthcheck-proxy \
  --engine_socket=/var/run/seesaw/engine \
  --rust_socket=/var/run/seesaw/healthcheck-proxy.sock
```

**Expected output:**
```
INFO Seesaw Healthcheck RPC Proxy starting
INFO Connected to Rust server at /var/run/seesaw/healthcheck-proxy.sock
INFO Rust server ready
INFO Getting healthchecks from engine...
```

### Mock Engine Test (without running Engine)

```bash
# Create a simple test script that sends configs via socket
cat > test_proxy.sh <<'EOF'
#!/bin/bash
# Sends test config to Rust server

SOCKET="/var/run/seesaw/healthcheck-proxy.sock"

# Wait for socket
while [ ! -S "$SOCKET" ]; do
  sleep 0.1
done

# Connect and send test config
{
  # Read Ready message
  read line
  echo "Received: $line"

  # Send UpdateConfigs
  cat <<JSON
{"type":"update_configs","configs":[{"id":1,"interval":"5s","timeout":"1s","retries":2,"checker_type":"tcp","ip":"127.0.0.1","port":8080}]}
JSON

  sleep 1
} | nc -U "$SOCKET"
EOF

chmod +x test_proxy.sh
./test_proxy.sh
```

## Deployment

### Systemd Service Files

#### Rust Server: `/etc/systemd/system/seesaw-healthcheck-rust.service`

```ini
[Unit]
Description=Seesaw Healthcheck Rust Server
After=network.target
Before=seesaw-healthcheck-proxy.service

[Service]
Type=simple
User=seesaw
Group=seesaw
Environment="RUST_LOG=info"
ExecStart=/usr/local/bin/healthcheck-server
Restart=always
RestartSec=5
RuntimeDirectory=seesaw
RuntimeDirectoryMode=0755

[Install]
WantedBy=multi-user.target
```

#### Go Proxy: `/etc/systemd/system/seesaw-healthcheck-proxy.service`

```ini
[Unit]
Description=Seesaw Healthcheck RPC Proxy
After=network.target seesaw-healthcheck-rust.service seesaw-engine.service
Requires=seesaw-healthcheck-rust.service

[Service]
Type=simple
User=seesaw
Group=seesaw
ExecStart=/usr/local/bin/healthcheck-proxy \
  --engine_socket=/var/run/seesaw/engine \
  --rust_socket=/var/run/seesaw/healthcheck-proxy.sock \
  --logtostderr
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### Installation Steps

```bash
# 1. Build binaries (see Building section)

# 2. Install binaries
sudo cp rust/target/release/healthcheck-server /usr/local/bin/
sudo cp bin/healthcheck-proxy /usr/local/bin/
sudo chmod +x /usr/local/bin/healthcheck-server
sudo chmod +x /usr/local/bin/healthcheck-proxy

# 3. Create seesaw user (if not exists)
sudo useradd -r -s /bin/false seesaw

# 4. Create runtime directory
sudo mkdir -p /var/run/seesaw
sudo chown seesaw:seesaw /var/run/seesaw

# 5. Install systemd services
sudo cp docs/systemd/seesaw-healthcheck-rust.service /etc/systemd/system/
sudo cp docs/systemd/seesaw-healthcheck-proxy.service /etc/systemd/system/
sudo systemctl daemon-reload

# 6. Enable and start services
sudo systemctl enable seesaw-healthcheck-rust
sudo systemctl enable seesaw-healthcheck-proxy
sudo systemctl start seesaw-healthcheck-rust
sudo systemctl start seesaw-healthcheck-proxy

# 7. Verify status
sudo systemctl status seesaw-healthcheck-rust
sudo systemctl status seesaw-healthcheck-proxy
```

### Configuration (Optional)

The Rust healthcheck server supports optional YAML-based configuration. If no configuration file is provided, the server uses sensible defaults.

#### Quick Start - No Configuration Needed

The server works out-of-the-box with defaults:
- Socket path: `/var/run/seesaw/healthcheck-proxy.sock`
- Batching: 100ms delay, 100 notifications per batch
- Polling: 500ms monitor interval
- Logging: Info level, text format

#### Creating a Configuration File

To customize behavior, create a YAML configuration file:

```bash
# Create system-wide configuration
sudo mkdir -p /etc/seesaw
sudo tee /etc/seesaw/healthcheck-server.yaml > /dev/null <<EOF
server:
  proxy_socket: "/var/run/seesaw/healthcheck-proxy.sock"

batching:
  delay: 100ms
  max_size: 100

channels:
  notification: 1000
  config_update: 10
  proxy_message: 10

manager:
  monitor_interval: 500ms

logging:
  level: "info"
  format: "json"
EOF
```

#### Configuration File Locations

The server searches in priority order:
1. `/etc/seesaw/healthcheck-server.yaml` (recommended for production)
2. `~/.config/seesaw/healthcheck-server.yaml` (user-specific)
3. `./healthcheck-server.yaml` (current directory)

#### Example Configurations

Pre-built examples are available in `rust/crates/healthcheck-server/examples/`:

**Production (balanced)**:
```bash
sudo cp rust/crates/healthcheck-server/examples/healthcheck-server-production.yaml \
        /etc/seesaw/healthcheck-server.yaml
```

**High-volume (500+ healthchecks)**:
```bash
sudo cp rust/crates/healthcheck-server/examples/healthcheck-server-high-volume.yaml \
        /etc/seesaw/healthcheck-server.yaml
```

**Development (debug logging, fast polling)**:
```bash
cp rust/crates/healthcheck-server/examples/healthcheck-server-development.yaml \
   ./healthcheck-server.yaml
```

#### Validating Configuration

After creating a config file, restart the server and check logs:

```bash
# Restart server
sudo systemctl restart seesaw-healthcheck-rust

# Check for successful load
sudo journalctl -u seesaw-healthcheck-rust -n 20 | grep -i config

# Expected output:
# "Configuration loaded successfully"

# If config is invalid:
# "Configuration error: <details>"
# "Using default configuration"
```

#### Configuration Documentation

For complete configuration reference and tuning guidelines:
- **[Configuration Reference](healthcheck-server-config.md)** - Complete field documentation
- **[Migration Guide](healthcheck-server-migration.md)** - Deployment migration guide
- **[Server README](../rust/crates/healthcheck-server/README.md)** - Quick start and examples

## Monitoring

### Check Service Status

```bash
# Status
sudo systemctl status seesaw-healthcheck-rust
sudo systemctl status seesaw-healthcheck-proxy

# Logs (journalctl)
sudo journalctl -u seesaw-healthcheck-rust -f
sudo journalctl -u seesaw-healthcheck-proxy -f

# Combined logs
sudo journalctl -u seesaw-healthcheck-rust -u seesaw-healthcheck-proxy -f
```

### Key Metrics to Monitor

1. **Socket Connection Status**
   - Check if `/var/run/seesaw/healthcheck-proxy.sock` exists
   - Verify both processes connected

2. **Message Flow**
   - Go proxy logs: "Sent N healthcheck configs to Rust server"
   - Rust server logs: "Received N healthcheck configs from proxy"
   - Notification batches being sent

3. **Health Check Execution**
   - Monitor state transitions (Unknown → Healthy/Unhealthy)
   - Check notification delivery to Engine

4. **Resource Usage**
   ```bash
   # CPU and memory
   ps aux | grep -E "healthcheck-server|healthcheck-proxy"

   # Open files/sockets
   sudo lsof -p $(pgrep healthcheck-server)
   sudo lsof -p $(pgrep healthcheck-proxy)
   ```

### Expected Log Patterns

**Rust Server (healthy)**:
```
INFO healthcheck_server: Starting healthcheck server
INFO Proxy listener started socket=/var/run/seesaw/healthcheck-proxy.sock
INFO Go proxy connected
INFO Sent Ready message to Go proxy
INFO All tasks spawned, server running
INFO Received 10 healthcheck configs from proxy
INFO id=1 old_state=Unknown new_state=Unhealthy: Health state changed
```

**Go Proxy (healthy)**:
```
INFO Seesaw Healthcheck RPC Proxy starting
INFO Connected to Rust server
INFO Rust server ready
INFO Engine returned 10 healthchecks
INFO Sent 10 healthcheck configs to Rust server
```

### Prometheus Metrics (Optional)

The Rust healthcheck server can expose comprehensive Prometheus metrics for production monitoring.

#### Enable Metrics

Add to `/etc/seesaw/healthcheck-server.yaml`:

```yaml
metrics:
  enabled: true
  listen_addr: "0.0.0.0:9090"
```

Restart the service:

```bash
sudo systemctl restart seesaw-healthcheck-rust
```

#### Verify Metrics Endpoint

```bash
# Test metrics endpoint
curl http://localhost:9090/metrics

# Sample output:
# healthcheck_checks_total{id="1",type="tcp",result="success"} 142
# healthcheck_response_time_seconds_bucket{id="1",type="tcp",le="0.01"} 120
# healthcheck_monitors_active 10
# healthcheck_proxy_connected 1
```

#### Configure Prometheus

Add scrape configuration to Prometheus (`prometheus.yml`):

```yaml
scrape_configs:
  - job_name: 'seesaw-healthcheck'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
```

Reload Prometheus:

```bash
# Send SIGHUP to reload config
sudo kill -HUP $(pgrep prometheus)

# Or restart service
sudo systemctl reload prometheus
```

#### Verify Prometheus Scraping

1. Open Prometheus UI: `http://prometheus-server:9090`
2. Navigate to **Status → Targets**
3. Verify `seesaw-healthcheck` target shows **UP**

#### Import Grafana Dashboard

1. Open Grafana
2. Navigate to **Dashboards → Import**
3. Upload file: `/path/to/seesaw/docs/healthcheck-server-grafana-dashboard.json`
4. Select Prometheus data source
5. Click **Import**

#### Example Alert Rules

Create `/etc/prometheus/rules/seesaw-healthcheck.yml`:

```yaml
groups:
  - name: seesaw_healthcheck
    interval: 30s
    rules:
      # Alert on proxy disconnect
      - alert: HealthcheckProxyDisconnected
        expr: healthcheck_proxy_connected == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Healthcheck server proxy disconnected"

      # Alert on low success rate
      - alert: HealthcheckLowSuccessRate
        expr: |
          (
            rate(healthcheck_checks_total{result="success"}[5m])
            /
            rate(healthcheck_checks_total[5m])
          ) < 0.9
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Healthcheck {{ $labels.id }} has low success rate"

      # Alert on flapping
      - alert: HealthcheckFlapping
        expr: |
          sum by (id) (
            rate(healthcheck_state_transitions_total[5m])
          ) > 0.1
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "Healthcheck {{ $labels.id }} is flapping"
```

Reference Prometheus rules in `prometheus.yml`:

```yaml
rule_files:
  - "/etc/prometheus/rules/seesaw-healthcheck.yml"
```

#### Key Metrics to Monitor

**Healthcheck Performance:**
- `healthcheck_checks_total` - Success/failure counts
- `healthcheck_response_time_seconds` - Response time distribution
- `healthcheck_state` - Current health state (0=unknown, 1=healthy, 2=unhealthy)

**System Health:**
- `healthcheck_monitors_active` - Number of active monitors
- `healthcheck_proxy_connected` - Proxy connection status
- `healthcheck_batch_size` - Notification batch sizes

**Errors:**
- `healthcheck_errors_total` - Error counts by type
- `healthcheck_state_transitions_total` - State change frequency (detect flapping)

**Example PromQL Queries:**

```promql
# Overall success rate
rate(healthcheck_checks_total{result="success"}[5m])
  /
rate(healthcheck_checks_total[5m])

# P95 response time
histogram_quantile(0.95,
  rate(healthcheck_response_time_seconds_bucket[5m])
)

# Unhealthy target count
count(healthcheck_state == 2)

# Average batch size
rate(healthcheck_batch_size_sum[5m])
  /
rate(healthcheck_batch_size_count[5m])
```

#### Performance Impact

- **CPU**: < 1% overhead when enabled
- **Memory**: ~500 KB for typical deployment
- **Network**: ~5-10 KB per scrape (every 15-60s)

#### Complete Documentation

See **[Metrics Reference Guide](healthcheck-server-metrics.md)** for:
- Complete metric family list
- Grafana dashboard panels
- Alert rule examples
- Troubleshooting guide

## Troubleshooting

### Rust Server Won't Start

**Symptom**: Rust server exits immediately

**Check**:
```bash
# Run manually to see error
./rust/target/debug/healthcheck-server

# Common issues:
# 1. Socket already exists
rm -f /var/run/seesaw/healthcheck-proxy.sock

# 2. Permission denied
sudo chown $(whoami) /var/run/seesaw
```

### Go Proxy Can't Connect

**Symptom**: "Failed to connect to Rust server"

**Solutions**:
```bash
# 1. Check Rust server is running
ps aux | grep healthcheck-server

# 2. Check socket exists
ls -l /var/run/seesaw/healthcheck-proxy.sock

# 3. Check permissions
sudo chmod 666 /var/run/seesaw/healthcheck-proxy.sock

# 4. Check socket path matches
# Go: --rust_socket=/var/run/seesaw/healthcheck-proxy.sock
# Rust: proxy_socket in ServerConfig::default()
```

### No Healthcheck Configs

**Symptom**: "Sent 0 healthcheck configs to Rust server"

**Check**:
```bash
# 1. Verify Engine is running
sudo systemctl status seesaw-engine

# 2. Check Engine socket
ls -l /var/run/seesaw/engine

# 3. Test Engine RPC manually
# (use seesawctl or direct RPC test)
```

### Notifications Not Reaching Engine

**Symptom**: Health state changes in Rust, but Engine doesn't see them

**Debug**:
```bash
# 1. Check notification logs in Go proxy
sudo journalctl -u seesaw-healthcheck-proxy | grep -i notification

# 2. Enable debug logging
# Rust: RUST_LOG=debug
# Go: Add --v=2 flag

# 3. Check Engine RPC connection
# Go proxy should log "SeesawEngine.HealthState" calls
```

### High CPU Usage

**Symptom**: Rust server using excessive CPU

**Investigate**:
```bash
# 1. Check number of monitors
sudo journalctl -u seesaw-healthcheck-rust | grep "Received.*configs"

# 2. Check healthcheck intervals
# Short intervals (< 1s) with many checks can cause high CPU

# 3. Profile (requires debug build)
# Use perf, flamegraph, or cargo-flamegraph
```

## Performance Validation

### Expected Performance

Based on Phase 3 benchmarks:

| Metric | Target | How to Measure |
|--------|--------|----------------|
| Pure Rust check latency | ~42µs | See benchmarks below |
| Socket + JSON overhead | ~10µs | End-to-end - pure Rust |
| Total latency | ~52µs | End-to-end measurement |
| Improvement vs FFI | 6.3x | 325µs → 52µs |

### Running Benchmarks

```bash
# Rust healthcheck benchmarks
cd rust/crates/healthcheck
cargo bench

# Look for:
# tcp_check/tcp_connection_refused: ~42 µs
# http_check/http_connection_refused: ~47 µs
```

### Measuring End-to-End Latency

1. **Add timing to Go proxy** (temporary instrumentation):
   ```go
   start := time.Now()
   err := sendBatch(notifications)
   latency := time.Since(start)
   log.Infof("Notification batch latency: %v", latency)
   ```

2. **Monitor Rust logs** for check durations:
   ```
   INFO Manager: Check completed id=1 duration=45µs state=Healthy
   ```

3. **Compare with old implementation**:
   - Run old healthcheck server
   - Measure same metrics
   - Verify 6x improvement

### Load Testing

```bash
# Create many healthchecks to test throughput
# (requires Engine/test harness)

# Expected capacity:
# - 1000 healthchecks @ 5s interval = 200 checks/sec
# - At 50µs/check = 10ms total (1% CPU)
# - Should handle 10,000+ healthchecks easily
```

## Migration from Old Healthcheck Server

### Gradual Migration

1. **Run both servers in parallel** (different socket paths)
2. **Migrate subset of healthchecks** to new server
3. **Monitor for issues**
4. **Gradually increase traffic** to new server
5. **Decommission old server** once validated

### Rollback Plan

If issues arise:

```bash
# 1. Stop new servers
sudo systemctl stop seesaw-healthcheck-proxy
sudo systemctl stop seesaw-healthcheck-rust

# 2. Start old server
sudo systemctl start seesaw-healthcheck

# 3. Verify Engine connectivity
# Check Engine logs for healthcheck updates
```

## Configuration

### Rust Server Config

Default values in `ServerConfig::default()`:

```rust
ServerConfig {
    batch_delay: 100ms,        // Notification batching delay
    batch_size: 100,           // Max notifications per batch
    channel_size: 1000,        // Internal channel buffer
    max_failures: 10,          // Max send failures before giving up
    notify_interval: 15s,      // Status notification interval
    fetch_interval: 15s,       // (Unused in Rust, Go handles this)
    retry_delay: 2s,           // Retry delay on failures
    proxy_socket: "/var/run/seesaw/healthcheck-proxy.sock",
}
```

To customize, modify `rust/crates/healthcheck-server/src/main.rs`:

```rust
let mut config = ServerConfig::default();
config.batch_size = 50;        // Smaller batches
config.batch_delay = Duration::from_millis(50);  // Faster batching
```

### Go Proxy Config

Command-line flags:

```bash
./healthcheck-proxy \
  --engine_socket=/custom/path/engine \
  --rust_socket=/custom/path/healthcheck.sock \
  --logtostderr \
  --v=1
```

## Best Practices

1. **Always run Rust server first**, then Go proxy (systemd handles this)
2. **Monitor socket file** - if it doesn't exist, something is wrong
3. **Check logs regularly** for errors or warnings
4. **Set appropriate log levels**:
   - Production: `RUST_LOG=info`
   - Debug: `RUST_LOG=debug` or `RUST_LOG=healthcheck_server=debug`
5. **Use systemd for auto-restart** - both services should restart on failure
6. **Monitor resource usage** - should be low (< 1% CPU for typical load)
7. **Test thoroughly** before production deployment

## Support

For issues or questions:

1. Check logs: `sudo journalctl -u seesaw-healthcheck-*`
2. Review [PHASE4.1_COMPLETION.md](PHASE4.1_COMPLETION.md) for architecture details
3. Run integration tests: `cargo test -p healthcheck-server`
4. File issues in the Seesaw repository

## Version History

- **v0.1.0** (Phase 4.1): Initial hybrid implementation
  - Basic RPC proxy functionality
  - Core health checking in Rust
  - Unix socket communication
  - 17 integration tests passing
