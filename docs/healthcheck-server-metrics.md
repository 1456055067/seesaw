# Healthcheck Server Metrics Reference

Complete reference for Prometheus metrics exported by the Rust healthcheck server.

## Table of Contents

- [Overview](#overview)
- [Enabling Metrics](#enabling-metrics)
- [Metric Families](#metric-families)
  - [Per-Healthcheck Metrics](#per-healthcheck-metrics)
  - [System-Wide Metrics](#system-wide-metrics)
  - [Resource Metrics](#resource-metrics)
- [Labels](#labels)
- [Example Queries](#example-queries)
- [Grafana Dashboard](#grafana-dashboard)
- [Prometheus Configuration](#prometheus-configuration)
- [Performance Impact](#performance-impact)
- [Troubleshooting](#troubleshooting)

## Overview

The healthcheck server exposes Prometheus metrics via an HTTP endpoint at `/metrics`. These metrics provide comprehensive observability into:

- **Healthcheck execution**: Success/failure rates, response times, state transitions
- **System performance**: Batch processing, notification latency, monitor counts
- **Resource utilization**: Channel depths, task durations
- **Error tracking**: Parse errors, socket I/O errors

Metrics follow Prometheus naming conventions and best practices:
- Counter names end with `_total`
- Histogram names end with appropriate units (`_seconds`, `_bytes`)
- Gauge names describe current state
- All metrics prefixed with `healthcheck_`

## Enabling Metrics

### Configuration

Add to your `healthcheck-server.yaml`:

```yaml
metrics:
  enabled: true
  listen_addr: "0.0.0.0:9090"
```

### Default Settings

- **Enabled**: `false` (opt-in)
- **Listen address**: `127.0.0.1:9090`
- **Histogram buckets**: Optimized defaults for healthcheck scenarios

### Custom Histogram Buckets

For specialized deployments, customize bucket boundaries:

```yaml
metrics:
  enabled: true
  listen_addr: "0.0.0.0:9090"

  # Response time buckets (seconds)
  response_time_buckets: [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]

  # Batch delay buckets (seconds)
  batch_delay_buckets: [0.01, 0.05, 0.1, 0.25, 0.5, 1.0]

  # Batch size buckets (count)
  batch_size_buckets: [1, 10, 50, 100, 500, 1000, 5000]
```

## Metric Families

### Per-Healthcheck Metrics

These metrics are labeled with `id` and `type` to track individual healthchecks.

#### `healthcheck_checks_total`

**Type**: Counter
**Description**: Total number of healthchecks executed
**Labels**:
- `id`: Healthcheck ID (e.g., "1", "42")
- `type`: Checker type (`tcp`, `http`, `dns`)
- `result`: Check result (`success`, `failure`)

**Example**:
```
healthcheck_checks_total{id="1",type="tcp",result="success"} 142
healthcheck_checks_total{id="1",type="tcp",result="failure"} 3
```

**Use cases**:
- Calculate success rate: `rate(healthcheck_checks_total{result="success"}[5m]) / rate(healthcheck_checks_total[5m])`
- Alert on high failure rate
- Track check volume per healthcheck

---

#### `healthcheck_response_time_seconds`

**Type**: Histogram
**Description**: Healthcheck response time distribution
**Labels**:
- `id`: Healthcheck ID
- `type`: Checker type

**Buckets**: `[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]`

**Example**:
```
healthcheck_response_time_seconds_bucket{id="1",type="tcp",le="0.01"} 120
healthcheck_response_time_seconds_bucket{id="1",type="tcp",le="0.1"} 145
healthcheck_response_time_seconds_sum{id="1",type="tcp"} 12.5
healthcheck_response_time_seconds_count{id="1",type="tcp"} 145
```

**Use cases**:
- P95 response time: `histogram_quantile(0.95, rate(healthcheck_response_time_seconds_bucket[5m]))`
- Alert on slow responses
- Track response time trends

---

#### `healthcheck_state`

**Type**: Gauge
**Description**: Current healthcheck state (0=unknown, 1=healthy, 2=unhealthy)
**Labels**:
- `id`: Healthcheck ID
- `type`: Checker type

**Example**:
```
healthcheck_state{id="1",type="tcp"} 1
healthcheck_state{id="2",type="http"} 2
```

**Use cases**:
- Dashboard showing current health status
- Count unhealthy targets: `count(healthcheck_state == 2)`
- Alert when specific healthcheck goes unhealthy

---

#### `healthcheck_consecutive_successes`

**Type**: Gauge
**Description**: Current consecutive success count for healthcheck
**Labels**:
- `id`: Healthcheck ID
- `type`: Checker type

**Example**:
```
healthcheck_consecutive_successes{id="1",type="tcp"} 15
```

**Use cases**:
- Track stability after recovery
- Identify flapping healthchecks (low consecutive counts)

---

#### `healthcheck_consecutive_failures`

**Type**: Gauge
**Description**: Current consecutive failure count for healthcheck
**Labels**:
- `id`: Healthcheck ID
- `type`: Checker type

**Example**:
```
healthcheck_consecutive_failures{id="2",type="http"} 3
```

**Use cases**:
- Alert on persistent failures
- Identify degraded backends

---

#### `healthcheck_state_transitions_total`

**Type**: Counter
**Description**: Total state transitions
**Labels**:
- `id`: Healthcheck ID
- `type`: Checker type
- `from`: Previous state (healthy, unhealthy, unknown)
- `to`: New state

**Example**:
```
healthcheck_state_transitions_total{id="1",type="tcp",from="healthy",to="unhealthy"} 2
healthcheck_state_transitions_total{id="1",type="tcp",from="unhealthy",to="healthy"} 2
```

**Use cases**:
- Flap detection: `rate(healthcheck_state_transitions_total[5m]) > 0.1`
- Track recovery events
- Identify unstable backends

---

### System-Wide Metrics

These metrics track overall server behavior.

#### `healthcheck_monitors_active`

**Type**: Gauge
**Description**: Number of active healthcheck monitors
**Labels**: None

**Example**:
```
healthcheck_monitors_active 42
```

**Use cases**:
- Dashboard showing total monitors
- Alert on unexpected changes in monitor count
- Capacity planning

---

#### `healthcheck_notifications_batched_total`

**Type**: Counter
**Description**: Total notifications added to batches
**Labels**: None

**Example**:
```
healthcheck_notifications_batched_total 1523
```

**Use cases**:
- Track notification volume
- Calculate average batch size

---

#### `healthcheck_notifications_sent_total`

**Type**: Counter
**Description**: Total notification batches sent
**Labels**:
- `trigger`: Batch trigger reason (`size_limit`, `time_delay`)

**Example**:
```
healthcheck_notifications_sent_total{trigger="size_limit"} 12
healthcheck_notifications_sent_total{trigger="time_delay"} 98
```

**Use cases**:
- Understand batch behavior
- Tune batch size/delay parameters
- Alert on batch processing issues

---

#### `healthcheck_batch_size`

**Type**: Histogram
**Description**: Distribution of notification batch sizes
**Labels**: None

**Buckets**: `[1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000]`

**Example**:
```
healthcheck_batch_size_bucket{le="10"} 45
healthcheck_batch_size_bucket{le="100"} 98
healthcheck_batch_size_sum 3420
healthcheck_batch_size_count 110
```

**Use cases**:
- Average batch size: `rate(healthcheck_batch_size_sum[5m]) / rate(healthcheck_batch_size_count[5m])`
- Optimize batch_size configuration

---

#### `healthcheck_batch_delay_seconds`

**Type**: Histogram
**Description**: Actual delay before sending batch
**Labels**: None

**Buckets**: `[0.01, 0.025, 0.05, 0.075, 0.1, 0.15, 0.2, 0.25, 0.5, 1.0]`

**Example**:
```
healthcheck_batch_delay_seconds_bucket{le="0.1"} 88
healthcheck_batch_delay_seconds_sum 9.2
healthcheck_batch_delay_seconds_count 110
```

**Use cases**:
- P99 batch delay: `histogram_quantile(0.99, rate(healthcheck_batch_delay_seconds_bucket[5m]))`
- Verify batching delay configuration
- Alert on excessive delays

---

#### `healthcheck_proxy_connected`

**Type**: Gauge
**Description**: Proxy connection status (1=connected, 0=disconnected)
**Labels**: None

**Example**:
```
healthcheck_proxy_connected 1
```

**Use cases**:
- Alert when proxy disconnects
- Track connection uptime
- Dashboard connectivity status

---

#### `healthcheck_config_updates_total`

**Type**: Counter
**Description**: Total configuration updates received
**Labels**: None

**Example**:
```
healthcheck_config_updates_total 15
```

**Use cases**:
- Track configuration change frequency
- Correlate config changes with issues

---

#### `healthcheck_errors_total`

**Type**: Counter
**Description**: Total errors by type
**Labels**:
- `type`: Error type (`parse`, `socket_io`, `channel`)

**Example**:
```
healthcheck_errors_total{type="parse"} 2
healthcheck_errors_total{type="socket_io"} 0
```

**Use cases**:
- Alert on error rate increase
- Track error types for debugging
- Identify communication issues

---

### Resource Metrics

These metrics track internal resource utilization.

#### `healthcheck_monitor_task_duration_seconds`

**Type**: Histogram
**Description**: Duration of manager monitor loop iterations
**Labels**: None

**Buckets**: Exponential buckets

**Example**:
```
healthcheck_monitor_task_duration_seconds_bucket{le="0.1"} 120
healthcheck_monitor_task_duration_seconds_sum 8.5
healthcheck_monitor_task_duration_seconds_count 125
```

**Use cases**:
- Monitor task performance
- Alert on slow iterations (CPU saturation)
- Identify performance degradation

---

## Labels

### Common Labels

| Label | Description | Example Values |
|-------|-------------|----------------|
| `id` | Healthcheck unique identifier | `"1"`, `"42"`, `"100"` |
| `type` | Checker type | `"tcp"`, `"http"`, `"dns"` |
| `result` | Check execution result | `"success"`, `"failure"` |
| `trigger` | Batch send trigger | `"size_limit"`, `"time_delay"` |
| `from` | Previous state | `"healthy"`, `"unhealthy"`, `"unknown"` |
| `to` | New state | `"healthy"`, `"unhealthy"`, `"unknown"` |
| `type` (error) | Error category | `"parse"`, `"socket_io"`, `"channel"` |

### Label Cardinality

**Estimated cardinality** (per deployment):
- 100 healthchecks × 3 types × 2 results = **600 time series** (checks_total)
- 100 healthchecks × 3 types × 12 buckets = **3,600 time series** (response_time histogram)
- System-wide metrics: **~50 time series**

**Total**: ~4,500-5,000 time series (low to medium cardinality, suitable for Prometheus)

## Example Queries

### Healthcheck Success Rate

Overall success rate (5-minute window):
```promql
rate(healthcheck_checks_total{result="success"}[5m])
  /
rate(healthcheck_checks_total[5m])
```

Per healthcheck success rate:
```promql
rate(healthcheck_checks_total{result="success",id="1"}[5m])
  /
rate(healthcheck_checks_total{id="1"}[5m])
```

### Response Time Percentiles

P95 response time across all healthchecks:
```promql
histogram_quantile(0.95,
  rate(healthcheck_response_time_seconds_bucket[5m])
)
```

P99 response time for specific healthcheck:
```promql
histogram_quantile(0.99,
  rate(healthcheck_response_time_seconds_bucket{id="1"}[5m])
)
```

### Unhealthy Targets

Count of currently unhealthy healthchecks:
```promql
count(healthcheck_state == 2)
```

List unhealthy targets:
```promql
healthcheck_state{state="unhealthy"} == 2
```

### Flapping Detection

Healthchecks with high state transition rate (potential flapping):
```promql
sum by (id, type) (
  rate(healthcheck_state_transitions_total[5m])
) > 0.1
```

### Batch Processing

Average batch size:
```promql
rate(healthcheck_batch_size_sum[5m])
  /
rate(healthcheck_batch_size_count[5m])
```

Batches triggered by size limit vs time delay:
```promql
sum by (trigger) (
  rate(healthcheck_notifications_sent_total[5m])
)
```

### Error Rates

Total error rate:
```promql
sum(rate(healthcheck_errors_total[5m]))
```

Error rate by type:
```promql
sum by (type) (
  rate(healthcheck_errors_total[5m])
)
```

## Grafana Dashboard

A complete Grafana dashboard JSON is provided in `docs/healthcheck-server-grafana-dashboard.json`.

### Dashboard Panels

**Overview Row**:
- Active Monitors (gauge)
- Overall Success Rate (gauge)
- Unhealthy Targets (gauge)
- Proxy Connected (gauge)

**Healthcheck Performance Row**:
- Check Rate (graph): `rate(healthcheck_checks_total[5m])`
- Success Rate by Type (graph): Success rate grouped by checker type
- P95 Response Time (graph): Response time percentiles over time
- Slow Healthchecks (table): Healthchecks with P95 > threshold

**State Transitions Row**:
- State Transitions (graph): Transition rate over time
- Current States (pie chart): Distribution of healthy/unhealthy/unknown
- Flapping Healthchecks (table): High transition rate healthchecks

**Batch Processing Row**:
- Batch Size (graph): Average batch size over time
- Batch Delay (graph): P95 batch delay
- Batch Trigger (pie chart): Size limit vs time delay triggers
- Notification Rate (graph): Batched vs sent rates

**Errors & Resources Row**:
- Error Rate (graph): Errors by type
- Monitor Task Duration (graph): P95 task duration
- Config Updates (graph): Configuration change rate

### Installing the Dashboard

1. Open Grafana
2. Navigate to Dashboards → Import
3. Upload `docs/healthcheck-server-grafana-dashboard.json`
4. Select your Prometheus data source
5. Click Import

## Prometheus Configuration

### Scrape Configuration

Add to `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'healthcheck-server'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
    scrape_timeout: 10s
```

### Kubernetes Service Discovery

For Kubernetes deployments:

```yaml
scrape_configs:
  - job_name: 'healthcheck-server'
    kubernetes_sd_configs:
      - role: pod
    relabel_configs:
      - source_labels: [__meta_kubernetes_pod_label_app]
        action: keep
        regex: healthcheck-server
      - source_labels: [__meta_kubernetes_pod_container_port_number]
        action: keep
        regex: 9090
```

### Alerting Rules

Example alert rules in `prometheus-rules.yml`:

```yaml
groups:
  - name: healthcheck_server
    interval: 30s
    rules:
      # Alert when any healthcheck has low success rate
      - alert: HealthcheckFailureRate
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
          description: "Success rate is {{ $value | humanizePercentage }}"

      # Alert when proxy disconnects
      - alert: ProxyDisconnected
        expr: healthcheck_proxy_connected == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "Healthcheck server proxy disconnected"

      # Alert on flapping healthchecks
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
          description: "State transition rate: {{ $value }}/sec"

      # Alert on high error rate
      - alert: HealthcheckErrorRate
        expr: rate(healthcheck_errors_total[5m]) > 0.1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High error rate in healthcheck server"
          description: "Error rate: {{ $value }}/sec"
```

## Performance Impact

### CPU Overhead

- **Metrics disabled**: 0% overhead
- **Metrics enabled**: < 1% CPU overhead
- Atomic counter increments: ~5-10 ns per operation
- Histogram observations: ~50-100 ns per operation

### Memory Overhead

- **Base registry**: ~1 KB
- **Per metric family**: ~200 bytes
- **Per time series**: ~100 bytes
- **Estimated total** (100 healthchecks): ~500 KB

### Network Overhead

- Scrape size: ~5-10 KB per scrape (compressed)
- Scrape frequency: Typically 15-60 seconds
- Bandwidth: < 1 KB/s average

## Troubleshooting

### Metrics endpoint not accessible

**Symptom**: `curl http://localhost:9090/metrics` fails

**Solutions**:
1. Check if metrics are enabled in config:
   ```yaml
   metrics:
     enabled: true
   ```

2. Verify listen address:
   ```bash
   ss -tlnp | grep 9090
   ```

3. Check firewall rules:
   ```bash
   sudo iptables -L | grep 9090
   ```

4. Review logs for startup errors:
   ```bash
   journalctl -u healthcheck-server | grep -i metrics
   ```

---

### No data in Prometheus

**Symptom**: Metrics endpoint works but Prometheus shows no data

**Solutions**:
1. Verify Prometheus is scraping:
   - Open Prometheus UI → Status → Targets
   - Check healthcheck-server target status

2. Check scrape configuration:
   ```yaml
   scrape_configs:
     - job_name: 'healthcheck-server'
       static_configs:
         - targets: ['localhost:9090']
   ```

3. Verify network connectivity:
   ```bash
   curl http://localhost:9090/metrics
   ```

4. Check Prometheus logs:
   ```bash
   journalctl -u prometheus | grep healthcheck
   ```

---

### Missing specific metrics

**Symptom**: Some metrics appear but others don't

**Possible causes**:
1. **No activity yet**: Metrics only appear after events occur
   - `healthcheck_checks_total`: Requires checks to execute
   - `healthcheck_state_transitions_total`: Requires state changes
   - `healthcheck_batch_size`: Requires batches to be sent

2. **Configuration issue**: Metrics recording may be disabled in code
   - Check that components receive `metrics` parameter
   - Verify `Option<Arc<MetricsRegistry>>` is `Some(_)`

3. **Label cardinality**: Metrics with specific labels may not match query
   - Try querying without label filters first
   - Use `{__name__=~"healthcheck_.*"}` to see all metrics

---

### High cardinality warning

**Symptom**: Prometheus logs warn about high cardinality

**Solutions**:
1. Reduce number of healthchecks (cardinality is proportional to healthcheck count)
2. Verify no unbounded labels are being added
3. Review metric label usage in code
4. Consider metric relabeling in Prometheus to drop high-cardinality labels

---

### Slow scrapes

**Symptom**: Prometheus scrapes timeout or are very slow

**Solutions**:
1. Check metric count:
   ```bash
   curl http://localhost:9090/metrics | wc -l
   ```

2. Increase scrape timeout in Prometheus:
   ```yaml
   scrape_timeout: 30s
   ```

3. Reduce scrape frequency:
   ```yaml
   scrape_interval: 60s
   ```

4. Verify healthcheck server isn't CPU-bound:
   ```bash
   top -p $(pgrep healthcheck-server)
   ```

---

## See Also

- [Healthcheck Server README](../rust/crates/healthcheck-server/README.md) - Server documentation
- [Configuration Reference](healthcheck-server-config.md) - Full configuration guide
- [Deployment Guide](HEALTHCHECK_HYBRID_DEPLOYMENT.md) - Production deployment
- [Prometheus Best Practices](https://prometheus.io/docs/practices/) - Prometheus documentation
