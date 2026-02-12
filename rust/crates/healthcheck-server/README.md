# Healthcheck Server

High-performance Rust-based healthcheck server for Seesaw load balancer.

## Overview

The healthcheck server is a critical component of Seesaw's hybrid architecture, responsible for executing healthchecks and reporting status changes to the engine. It provides:

- **High Performance**: 6.3x throughput improvement over pure Go implementation
- **Multiple Checker Types**: TCP, HTTP/HTTPS, DNS
- **Efficient Batching**: Configurable notification aggregation
- **Unix Socket IPC**: Low-latency communication with Go proxy
- **Production Ready**: Comprehensive error handling and logging

## Architecture

```
┌──────────────────────────────────────────────────────┐
│           Healthcheck Server (Rust)                  │
│                                                      │
│  ┌──────────┐  ┌─────────┐  ┌──────────┐           │
│  │ Manager  │→ │Notifier │→ │ProxyComm │ ←─────┐   │
│  └──────────┘  └─────────┘  └──────────┘       │   │
│       ↓                           ↓             │   │
│  [Monitors]              [Unix Socket]          │   │
│                                                  │   │
└──────────────────────────────────────────────────┘  │
                                                       │
                                                       ↓
                                              ┌────────────────┐
                                              │  Go Proxy      │
                                              │                │
                                              │  Seesaw Engine │
                                              └────────────────┘
```

## Features

### Healthcheck Types

- **TCP**: Port connectivity checks
- **HTTP/HTTPS**: HTTP endpoint validation with expected status codes
- **DNS**: DNS resolution verification with expected IP addresses

### Performance

- Asynchronous healthcheck execution with Tokio
- Concurrent monitor management with DashMap
- Efficient notification batching
- Low-overhead Unix socket communication

### Reliability

- Configurable retry logic
- Graceful error handling
- Comprehensive logging with tracing
- Safe concurrent access to monitor state

## Configuration

The healthcheck server supports optional YAML-based configuration with comprehensive schema validation.

### Quick Start

**No configuration needed** - The server works out-of-the-box with sensible defaults:

```bash
cargo run -p healthcheck-server
```

**Optional configuration** - Create a config file to customize behavior:

```bash
# Create system-wide configuration
sudo mkdir -p /etc/seesaw
sudo vim /etc/seesaw/healthcheck-server.yaml
```

### Configuration File Locations

The server searches for configuration files in priority order:

1. `/etc/seesaw/healthcheck-server.yaml` (system-wide)
2. `~/.config/seesaw/healthcheck-server.yaml` (user-specific)
3. `./healthcheck-server.yaml` (current directory)

### Minimal Configuration Example

```yaml
# /etc/seesaw/healthcheck-server.yaml
server:
  proxy_socket: "/var/run/seesaw/healthcheck-proxy.sock"

logging:
  level: "info"
  format: "json"
```

### Example Configurations

Four pre-configured examples are provided for common scenarios:

- **`healthcheck-server-minimal.yaml`** - Minimal config with mostly defaults
- **`healthcheck-server-development.yaml`** - Fast polling, debug logging
- **`healthcheck-server-production.yaml`** - Balanced production settings
- **`healthcheck-server-high-volume.yaml`** - Large buffers for 500+ healthchecks

Find examples in: `examples/`

### Configuration Sections

```yaml
server:
  proxy_socket: "/var/run/seesaw/healthcheck-proxy.sock"

batching:
  delay: 100ms        # Notification batching delay (1ms - 10s)
  max_size: 100       # Maximum notifications per batch (1 - 10000)

channels:
  notification: 1000  # Notification channel buffer (10 - 100000)
  config_update: 10   # Config update channel buffer (1 - 1000)
  proxy_message: 10   # Proxy message channel buffer (1 - 1000)

manager:
  monitor_interval: 500ms  # Monitor polling interval (10ms - 60s)

logging:
  level: "info"       # Log level: error, warn, info, debug, trace
  format: "json"      # Log format: text, json
```

### Validation

All configuration values are validated at startup:

- **Type checking**: Field types must match schema
- **Range validation**: Numeric values must be within allowed ranges
- **Duration parsing**: Human-readable format (`100ms`, `5s`, `1m`)
- **Path validation**: Socket paths must be non-empty

**Error handling**: If validation fails, the server logs the error and uses built-in defaults.

### Documentation

- **[Configuration Reference](../../../docs/healthcheck-server-config.md)** - Complete configuration documentation
- **[Migration Guide](../../../docs/healthcheck-server-migration.md)** - How to migrate existing deployments

## Building

```bash
# Build the server
cargo build -p healthcheck-server --release

# Build with optimizations
RUSTFLAGS="-C target-cpu=native" cargo build -p healthcheck-server --release
```

## Running

```bash
# Run with default configuration
cargo run -p healthcheck-server

# Run with debug logging
RUST_LOG=debug cargo run -p healthcheck-server

# Run release build
./target/release/healthcheck-server
```

## Testing

```bash
# Run all tests
cargo test -p healthcheck-server

# Run with output
cargo test -p healthcheck-server -- --nocapture

# Run specific test
cargo test -p healthcheck-server test_manager_adds_new_healthchecks

# Run configuration tests only
cargo test -p healthcheck-server config
```

## Development

### Project Structure

```
healthcheck-server/
├── src/
│   ├── main.rs          # Entry point
│   ├── lib.rs           # Public API
│   ├── server.rs        # Server orchestration
│   ├── manager.rs       # Monitor lifecycle management
│   ├── notifier.rs      # Notification batching
│   ├── proxy.rs         # Unix socket communication
│   ├── types.rs         # Type definitions
│   └── config.rs        # Configuration loading and validation
├── tests/               # Integration tests
│   ├── manager_test.rs
│   ├── notifier_test.rs
│   └── proxy_test.rs
└── examples/            # Example configurations
    ├── healthcheck-server-minimal.yaml
    ├── healthcheck-server-development.yaml
    ├── healthcheck-server-production.yaml
    └── healthcheck-server-high-volume.yaml
```

### Key Components

- **Manager**: Manages healthcheck monitor lifecycle (creation, updates, deletion)
- **Notifier**: Batches notifications and sends to proxy
- **ProxyComm**: Unix socket communication with Go proxy
- **Monitor**: Individual healthcheck execution (from `healthcheck` crate)

### Adding New Features

1. **Add configuration field** to `src/config.rs` with validation
2. **Update ServerConfig** in `src/types.rs`
3. **Thread through components** (main → server → component)
4. **Add tests** for the new functionality
5. **Update documentation** with new configuration options
6. **Create example** showing the new feature

## Performance Tuning

### Low-Volume (< 100 healthchecks)

Use defaults - no tuning needed.

### Medium-Volume (100-500 healthchecks)

```yaml
batching:
  delay: 100ms
  max_size: 100

channels:
  notification: 1000
```

### High-Volume (500+ healthchecks)

```yaml
batching:
  delay: 250ms
  max_size: 1000

channels:
  notification: 10000
  config_update: 100
  proxy_message: 100
```

### Latency-Sensitive

```yaml
batching:
  delay: 50ms

manager:
  monitor_interval: 250ms
```

## Deployment

See the [Hybrid Deployment Guide](../../../docs/HEALTHCHECK_HYBRID_DEPLOYMENT.md) for complete deployment instructions.

### Systemd Service

```ini
[Unit]
Description=Seesaw Healthcheck Server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/healthcheck-server
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

### Container Deployment

```dockerfile
FROM rust:1.75 AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p healthcheck-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /build/target/release/healthcheck-server /usr/local/bin/
COPY healthcheck-server.yaml /config/healthcheck-server.yaml
CMD ["/usr/local/bin/healthcheck-server"]
```

## Monitoring

### Prometheus Metrics

The healthcheck server exposes comprehensive Prometheus metrics for production monitoring:

**Enable metrics in configuration:**

```yaml
metrics:
  enabled: true
  listen_addr: "0.0.0.0:9090"
```

**Key metric families:**

- **Per-healthcheck metrics**: Check success/failure rates, response times, state transitions
- **System-wide metrics**: Active monitors, batch processing, notification rates
- **Resource metrics**: Channel depths, task durations, error rates

**Access metrics endpoint:**

```bash
curl http://localhost:9090/metrics
```

### Quick Start: Prometheus + Grafana Stack

Start the complete monitoring stack with Docker Compose:

```bash
cd rust/crates/healthcheck-server

# Start Prometheus and Grafana
docker-compose up -d

# Access monitoring tools
# - Prometheus UI: http://localhost:9091
# - Grafana Dashboard: http://localhost:3000 (admin/admin)
```

The stack includes:
- **Prometheus**: Pre-configured to scrape healthcheck-server metrics
- **Grafana**: Auto-provisioned with "Healthcheck Server Metrics" dashboard
- **Persistent storage**: Volumes for data retention across restarts

**Verify stack is working:**

```bash
# Run integration test
./tests/monitoring_stack_test.sh
```

See **[MONITORING.md](./MONITORING.md)** for complete setup guide including:
- Configuration reference
- Troubleshooting common issues
- Custom dashboards and alerts
- Production deployment considerations

See **[Metrics Reference Guide](../../../docs/healthcheck-server-metrics.md)** for:
- Complete metric family list with descriptions
- Example PromQL queries
- Alerting rule examples
- Performance impact analysis (< 0.01% CPU overhead)

### OpenTelemetry Distributed Tracing

The healthcheck server supports OpenTelemetry for distributed tracing and performance analysis:

**Enable tracing in configuration:**

```yaml
telemetry:
  enabled: true
  service_name: "healthcheck-server"
  otlp_endpoint: "http://localhost:4317"  # Jaeger OTLP endpoint
  use_http: false  # Use gRPC (recommended)
  sampling_rate: 1.0  # Sample 100% (reduce in production)
```

**Quick Start with Jaeger:**

```bash
# Start Jaeger all-in-one
docker-compose -f docker-compose.jaeger.yml up -d

# Start healthcheck-server with telemetry enabled
cargo run -p healthcheck-server --release

# View traces in Jaeger UI
# http://localhost:16686
```

**Features:**
- **Distributed tracing**: End-to-end request flow visibility
- **Performance analysis**: Detailed timing breakdowns for healthcheck operations
- **Correlation**: Link traces with Prometheus metrics via trace IDs
- **Flexible backends**: Jaeger, Zipkin, or any OTLP-compatible collector

See **[OPENTELEMETRY.md](./OPENTELEMETRY.md)** for complete guide including:
- Configuration reference and examples
- Trace structure and span details
- Backend setup (Jaeger, Zipkin, OpenTelemetry Collector)
- Troubleshooting and best practices
- Performance impact analysis (< 0.5% CPU overhead)

### Log Messages

```
# Startup
"Starting healthcheck server"
"Configuration loaded successfully"
"All tasks spawned, server running"

# Configuration
"Loading configuration from: /etc/seesaw/healthcheck-server.yaml"
"No configuration file found, using defaults"

# Runtime
"Adding healthcheck" (with id and target)
"Removing healthcheck" (with id)
"Health state changed" (with id, old_state, new_state)

# Errors
"Configuration error: ..." (validation failures)
"Proxy task error" (communication issues)
"Failed to send notification" (channel issues)
```

## Troubleshooting

### Server won't start

1. Check logs: `journalctl -u healthcheck-server`
2. Verify configuration: Check for validation errors
3. Test socket permissions: Ensure proxy socket is accessible
4. Check port availability: Ensure no port conflicts

### Configuration not loading

1. Verify file exists: `ls -la /etc/seesaw/healthcheck-server.yaml`
2. Check permissions: File must be readable
3. Validate YAML syntax: Use `yamllint` or similar
4. Review logs: Look for "Configuration loaded successfully"

### High latency

1. Reduce `batching.delay` for faster notification delivery
2. Reduce `manager.monitor_interval` for faster detection
3. Check channel buffer depth - increase if near capacity
4. Monitor CPU usage - may need to reduce polling frequency

### High memory usage

1. Reduce channel buffer sizes if oversized
2. Reduce `batching.max_size` to limit batch memory
3. Check monitor count - may be too many healthchecks
4. Review monitor interval - very fast polling increases memory

## License

Apache License 2.0 - See [LICENSE](../../../LICENSE) for details.

## Contributing

See [CONTRIBUTING.md](../../../CONTRIBUTING.md) for development guidelines.

## See Also

- [Healthcheck Library](../healthcheck/README.md) - Core healthcheck implementation
- [Deployment Guide](../../../docs/HEALTHCHECK_HYBRID_DEPLOYMENT.md) - Complete deployment instructions
- [Configuration Reference](../../../docs/healthcheck-server-config.md) - Detailed configuration documentation
- [Metrics Reference](../../../docs/healthcheck-server-metrics.md) - Prometheus metrics guide
- [Migration Guide](../../../docs/healthcheck-server-migration.md) - Migration guide for existing deployments
