# Healthcheck Server Configuration Reference

## Overview

The healthcheck server supports YAML-based configuration with comprehensive schema validation. Configuration is completely optional - the server will run with built-in defaults if no configuration file is provided.

## Configuration File Locations

The server searches for configuration files in the following priority order:

1. `/etc/seesaw/healthcheck-server.yaml` (system-wide configuration)
2. `~/.config/seesaw/healthcheck-server.yaml` (user-specific configuration)
3. `./healthcheck-server.yaml` (current directory)

The first existing file found will be used. If no file is found, the server uses built-in defaults.

## Configuration Structure

The configuration is organized into six main sections:

```yaml
server:
  # Server settings (socket paths, etc.)

batching:
  # Notification batching behavior

channels:
  # Internal channel buffer sizes

manager:
  # Monitor lifecycle management

advanced:
  # Advanced tuning parameters (rarely changed)

logging:
  # Logging configuration
```

## Configuration Sections

### Server Settings

Controls core server behavior and external communication.

```yaml
server:
  proxy_socket: "/var/run/seesaw/healthcheck-proxy.sock"
```

**Fields:**

- `proxy_socket` (string): Unix socket path for communication with Go proxy
  - **Default**: `/var/run/seesaw/healthcheck-proxy.sock`
  - **Validation**: Must be non-empty and a valid path format
  - **Example**: `/tmp/healthcheck-proxy.sock` (development)

### Batching Settings

Controls how notifications are batched before sending to the Go proxy.

```yaml
batching:
  delay: 100ms
  max_size: 100
```

**Fields:**

- `delay` (duration): Maximum time to wait before sending a batch
  - **Default**: `100ms`
  - **Range**: `1ms` to `10s`
  - **Purpose**: Balance between latency and CPU efficiency
  - **Examples**: `50ms` (low latency), `250ms` (high throughput)

- `max_size` (integer): Maximum number of notifications per batch
  - **Default**: `100`
  - **Range**: `1` to `10000`
  - **Purpose**: Prevent unbounded batch growth
  - **Examples**: `10` (development), `1000` (high volume)

### Channel Settings

Controls internal tokio channel buffer sizes for async communication.

```yaml
channels:
  notification: 1000
  config_update: 10
  proxy_message: 10
```

**Fields:**

- `notification` (integer): Buffer size for healthcheck notifications
  - **Default**: `1000`
  - **Range**: `10` to `100000`
  - **Purpose**: Handle bursts of state changes
  - **Tuning**: Increase for high-volume deployments

- `config_update` (integer): Buffer size for configuration updates
  - **Default**: `10`
  - **Range**: `1` to `1000`
  - **Purpose**: Queue configuration changes
  - **Tuning**: Rarely needs adjustment

- `proxy_message` (integer): Buffer size for proxy messages
  - **Default**: `10`
  - **Range**: `1` to `1000`
  - **Purpose**: Queue messages from Go proxy
  - **Tuning**: Increase if config updates are frequent

### Manager Settings

Controls the monitor lifecycle manager behavior.

```yaml
manager:
  monitor_interval: 500ms
```

**Fields:**

- `monitor_interval` (duration): Polling interval for checking monitor health
  - **Default**: `500ms`
  - **Range**: `10ms` to `60s`
  - **Purpose**: Balance between responsiveness and CPU usage
  - **Examples**: `100ms` (development/testing), `1s` (resource-constrained)

### Metrics Settings

Controls Prometheus metrics exposition and monitoring.

```yaml
metrics:
  enabled: true
  listen_addr: "0.0.0.0:9090"
  response_time_buckets: [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]
  batch_delay_buckets: [0.01, 0.05, 0.1, 0.25, 0.5, 1.0]
  batch_size_buckets: [1, 10, 50, 100, 500, 1000, 5000]
```

**Fields:**

- `enabled` (boolean): Enable Prometheus metrics HTTP endpoint
  - **Default**: `false` (opt-in)
  - **Purpose**: Expose metrics for monitoring and alerting
  - **Production**: Set to `true` for observability

- `listen_addr` (string): HTTP server listen address for metrics endpoint
  - **Default**: `127.0.0.1:9090`
  - **Validation**: Must be non-empty, format `host:port`
  - **Examples**:
    - `0.0.0.0:9090` - Listen on all interfaces (production)
    - `127.0.0.1:9090` - Localhost only (development)
    - `10.0.1.5:9090` - Specific interface

- `response_time_buckets` (list of floats): Histogram buckets for healthcheck response times (in seconds)
  - **Default**: `[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]`
  - **Purpose**: Define response time histogram buckets for percentile calculations
  - **Tuning**: Adjust based on expected response time ranges
  - **Examples**:
    - `[0.001, 0.01, 0.1, 1.0]` - Coarse buckets
    - `[0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1]` - Fine-grained for fast checks

- `batch_delay_buckets` (list of floats): Histogram buckets for batch delays (in seconds)
  - **Default**: `[0.01, 0.025, 0.05, 0.075, 0.1, 0.15, 0.2, 0.25, 0.5, 1.0]`
  - **Purpose**: Track actual batch delay distribution
  - **Tuning**: Align with configured `batching.delay` value

- `batch_size_buckets` (list of floats): Histogram buckets for batch sizes (notification count)
  - **Default**: `[1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000]`
  - **Purpose**: Track notification batch size distribution
  - **Tuning**: Align with configured `batching.max_size` value
  - **High-volume**: Use `[1, 10, 50, 100, 250, 500, 1000, 2500, 5000, 10000]`

**Performance Impact:**

- **CPU**: < 1% when enabled (atomic operations, lock-free)
- **Memory**: ~500 KB for typical deployment (100 healthchecks)
- **Network**: ~5-10 KB per Prometheus scrape (15-60s interval)

**See Also:**

- [Metrics Reference Guide](healthcheck-server-metrics.md) - Complete metrics documentation
- [Prometheus Documentation](https://prometheus.io/docs/) - Prometheus project docs

### Advanced Settings

Advanced tuning parameters that rarely need adjustment.

```yaml
advanced:
  max_failures: 10
  notify_interval: 15s
  fetch_interval: 15s
  retry_delay: 2s
```

**Fields:**

- `max_failures` (integer): Maximum notification failures before giving up
  - **Default**: `10`
  - **Purpose**: Prevent infinite retry loops

- `notify_interval` (duration): Interval between status notifications
  - **Default**: `15s`
  - **Purpose**: Regular status updates

- `fetch_interval` (duration): Interval between config fetches
  - **Default**: `15s`
  - **Purpose**: Config refresh rate

- `retry_delay` (duration): Delay before retrying failed operations
  - **Default**: `2s`
  - **Purpose**: Backoff on failures

**Note**: These fields are currently unused in the implementation but reserved for future features. They are validated but have no runtime effect.

### Logging Settings

Controls logging behavior and format.

```yaml
logging:
  level: "info"
  format: "json"
```

**Fields:**

- `level` (string): Logging level
  - **Default**: `"info"`
  - **Valid values**: `"error"`, `"warn"`, `"info"`, `"debug"`, `"trace"`
  - **Note**: Overrides `RUST_LOG` environment variable if set

- `format` (string): Log output format
  - **Default**: `"text"`
  - **Valid values**: `"text"`, `"json"`
  - **Recommendation**: Use `"json"` in production for log aggregation

## Duration Format

All duration fields use the `humantime` format for readability:

- Milliseconds: `100ms`, `500ms`, `1000ms`
- Seconds: `1s`, `5s`, `30s`
- Minutes: `1m`, `5m`, `15m`
- Hours: `1h`, `2h`
- Combined: `1m30s`, `2h30m`

**Examples:**
```yaml
batching:
  delay: 100ms      # 100 milliseconds

manager:
  monitor_interval: 500ms   # 500 milliseconds

advanced:
  notify_interval: 15s      # 15 seconds
  retry_delay: 2s           # 2 seconds
```

## Complete Example

```yaml
# Production healthcheck-server configuration

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

advanced:
  max_failures: 10
  notify_interval: 15s
  fetch_interval: 15s
  retry_delay: 2s

logging:
  level: "info"
  format: "json"
```

## Validation

The server performs comprehensive validation at startup:

1. **YAML Parsing**: Ensures valid YAML syntax
2. **Type Checking**: Validates field types match schema
3. **Range Validation**: Ensures numeric values are within allowed ranges
4. **Duration Parsing**: Validates duration format and ranges
5. **Path Validation**: Ensures socket paths are non-empty

**Validation Errors:**

If validation fails, the server will:
- Print a detailed error message indicating which field failed and why
- Print "Using default configuration"
- Continue running with built-in defaults

**Example Error:**
```
Configuration error: Validation error: batching.delay: must be between 1ms and 10s
Using default configuration
```

## Performance Impact

Configuration loading occurs once at startup:
- YAML parsing: < 1ms
- Validation: < 1ms
- Total overhead: Negligible

There is no runtime performance impact from using configuration files.

## Example Configurations

The server includes four example configurations for common scenarios:

1. **`healthcheck-server-minimal.yaml`**: Minimal configuration with mostly defaults
2. **`healthcheck-server-development.yaml`**: Development settings (debug logging, fast polling)
3. **`healthcheck-server-production.yaml`**: Production settings (info logging, balanced performance)
4. **`healthcheck-server-high-volume.yaml`**: High-volume deployment (large buffers, large batches)

Find these in: `rust/crates/healthcheck-server/examples/`

## Troubleshooting

### Configuration Not Loading

**Problem**: Server says "No configuration file found"

**Solution**:
- Verify file exists at one of the search paths
- Check file permissions (must be readable)
- Verify filename is exactly `healthcheck-server.yaml`

### Validation Errors

**Problem**: "Validation error" message at startup

**Solution**:
- Read the error message carefully - it indicates which field failed
- Check the value is within the allowed range
- Verify duration format (e.g., `100ms` not `100`)
- Compare against the examples in this document

### YAML Parsing Errors

**Problem**: "Error parsing YAML" or similar message

**Solution**:
- Validate YAML syntax with a YAML linter
- Check indentation (YAML requires consistent spaces)
- Ensure no tabs are used (YAML forbids tabs)
- Verify quotes are balanced for string values

### Server Not Using Configuration

**Problem**: Server starts but doesn't use my configuration values

**Solution**:
- Check server logs for "Configuration loaded successfully" message
- Verify you're editing the correct file in the search path
- Ensure file has `.yaml` extension (not `.yml`)
- Try specifying full path: `HEALTHCHECK_CONFIG=/path/to/config.yaml healthcheck-server`

## See Also

- [Migration Guide](healthcheck-server-migration.md) - How to migrate existing deployments
- [Deployment Guide](HEALTHCHECK_HYBRID_DEPLOYMENT.md) - Complete deployment instructions
- Example configurations in `rust/crates/healthcheck-server/examples/`
