# Healthcheck Server Monitoring Guide

Complete guide for setting up Prometheus and Grafana to monitor the healthcheck server metrics.

## Quick Start

### 1. Prerequisites

- Docker and Docker Compose installed
- Healthcheck server built and configured
- Port 9090 available for healthcheck metrics endpoint
- Ports 9091 (Prometheus) and 3000 (Grafana) available

### 2. Start Monitoring Stack

```bash
cd /home/jwillman/projects/seesaw/rust/crates/healthcheck-server

# Start Prometheus and Grafana
docker-compose up -d

# Check containers are running
docker-compose ps

# View logs
docker-compose logs -f
```

### 3. Start Healthcheck Server

Ensure your `healthcheck-server.yaml` has metrics enabled:

```yaml
metrics:
  enabled: true
  listen_addr: "127.0.0.1:9090"
  response_time_buckets: [0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
  batch_delay_buckets: [0.01, 0.025, 0.05, 0.075, 0.1, 0.15, 0.2, 0.25, 0.5, 1.0]
  batch_size_buckets: [1, 5, 10, 25, 50, 100, 250, 500, 1000, 5000]
```

Start the server:

```bash
cd /home/jwillman/projects/seesaw/rust
cargo run -p healthcheck-server --release
```

### 4. Access Monitoring Tools

**Healthcheck Metrics Endpoint:**
- URL: http://localhost:9090/metrics
- Format: Prometheus text format
- Test: `curl http://localhost:9090/metrics`

**Prometheus UI:**
- URL: http://localhost:9091
- Username: N/A (no authentication)
- Features:
  - Query metrics using PromQL
  - View targets (Status → Targets)
  - Explore time series data
  - Test alert rules

**Grafana Dashboard:**
- URL: http://localhost:3000
- Username: `admin`
- Password: `admin` (change on first login)
- Dashboard: "Healthcheck Server Metrics" (auto-provisioned)

## Verification

### Check Metrics Endpoint

```bash
# Test healthcheck server metrics endpoint
curl http://localhost:9090/metrics

# Should see output like:
# TYPE healthcheck_checks_total counter
# healthcheck_checks_total{id="1",type="tcp",result="success"} 42
# ...
```

### Check Prometheus Scraping

1. Open Prometheus UI: http://localhost:9091
2. Go to **Status → Targets**
3. Verify `healthcheck-server` target shows **UP**
4. Click "show more" to see last scrape time and duration

### Query Metrics in Prometheus

Example queries:

```promql
# Total healthchecks
sum(healthcheck_checks_total)

# Success rate per healthcheck
sum(rate(healthcheck_checks_total{result="success"}[5m])) by (id, type)
/ sum(rate(healthcheck_checks_total[5m])) by (id, type)

# Active monitors
healthcheck_monitors_active

# 95th percentile response time
histogram_quantile(0.95, sum(rate(healthcheck_response_time_seconds_bucket[5m])) by (le, id, type))

# State transitions
rate(healthcheck_state_transitions_total[5m])
```

### View Grafana Dashboard

1. Open Grafana: http://localhost:3000
2. Login with `admin` / `admin`
3. Navigate to **Dashboards**
4. Open **Healthcheck Server Metrics**

The dashboard includes:
- **Overview Panel**: Active monitors, proxy status, total checks
- **Check Results**: Success/failure rates per healthcheck
- **Response Times**: Latency histograms and percentiles
- **State Tracking**: Current states, consecutive counts, transitions
- **Batch Processing**: Batch sizes, delays, trigger reasons
- **System Metrics**: Errors, config updates, task durations

## Configuration

### Prometheus Configuration

**File:** `prometheus.yml`

Key settings:

```yaml
global:
  scrape_interval: 15s      # How often to scrape (default: 15s)
  scrape_timeout: 10s       # Timeout for scraping (default: 10s)

scrape_configs:
  - job_name: 'healthcheck-server'
    static_configs:
      - targets:
          - 'host.docker.internal:9090'  # Healthcheck server metrics endpoint
```

**Adjustments:**

- **Scrape interval**: Decrease for higher resolution (e.g., `5s`), increase for lower load (e.g., `30s`)
- **Multiple instances**: Add more targets under `static_configs`
- **Relabeling**: Use `metric_relabel_configs` to filter or transform metrics

### Grafana Data Source

**File:** `grafana/provisioning/datasources/prometheus.yml`

Auto-configured on startup:

```yaml
datasources:
  - name: Prometheus
    type: prometheus
    url: http://prometheus:9090
    isDefault: true
```

### Grafana Dashboard

**File:** `grafana/dashboards/healthcheck-server-grafana-dashboard.json`

Auto-loaded on startup via provisioning configuration.

**Customization:**

1. Open dashboard in Grafana UI
2. Click **Settings** (gear icon)
3. Modify panels, queries, or layout
4. Click **Save dashboard**
5. Export JSON to update the file

## Troubleshooting

### Healthcheck Metrics Not Available

**Symptom:** `curl http://localhost:9090/metrics` fails

**Solutions:**
1. Check healthcheck server is running: `ps aux | grep healthcheck-server`
2. Verify metrics enabled in config: `grep -A 5 "metrics:" healthcheck-server.yaml`
3. Check server logs for errors: `journalctl -u healthcheck-server -f` or check stdout
4. Verify port 9090 is open: `netstat -tulpn | grep 9090`

### Prometheus Target Down

**Symptom:** Prometheus UI shows healthcheck-server target as **DOWN**

**Solutions:**
1. Check Prometheus can reach the endpoint:
   ```bash
   docker exec healthcheck-prometheus curl http://host.docker.internal:9090/metrics
   ```
2. On Linux, ensure `host.docker.internal` resolves:
   ```bash
   # Add to docker-compose.yml under prometheus service:
   extra_hosts:
     - "host.docker.internal:host-gateway"
   ```
3. Alternatively, use host network mode or bridge network with explicit IP
4. Check Prometheus logs: `docker-compose logs prometheus`

### No Data in Grafana

**Symptom:** Grafana dashboard shows "No data"

**Solutions:**
1. Verify Prometheus data source is working:
   - Go to **Connections → Data sources → Prometheus**
   - Click **Save & test**
   - Should show "Data source is working"
2. Check Prometheus has data:
   - Open Prometheus UI (http://localhost:9091)
   - Run query: `healthcheck_monitors_active`
   - Verify results appear
3. Check dashboard time range (top-right corner)
4. Verify healthcheck server has active monitors (metrics only appear with activity)

### Port Conflicts

**Symptom:** `docker-compose up` fails with port already in use

**Solutions:**
1. Healthcheck server metrics (9090):
   - Container uses 9091 for Prometheus to avoid conflict
2. Grafana (3000):
   - Change in `docker-compose.yml`: `ports: ["3001:3000"]`
3. Prometheus (9091):
   - Change in `docker-compose.yml`: `ports: ["9092:9090"]`

## Advanced Topics

### Production Deployment

**Persistence:**
- Volumes `prometheus-data` and `grafana-data` persist across restarts
- Backup these volumes regularly
- Consider external storage for production

**Security:**
1. Change Grafana admin password:
   ```bash
   docker exec -it healthcheck-grafana grafana-cli admin reset-admin-password <new-password>
   ```
2. Enable authentication on Prometheus (use reverse proxy with auth)
3. Use HTTPS with TLS certificates (via reverse proxy)
4. Restrict network access (firewall rules, network policies)

**Scalability:**
- Use Prometheus federation for multiple healthcheck servers
- Configure Prometheus remote write for long-term storage (e.g., Thanos, Cortex)
- Set up Alertmanager for notifications

### Alert Rules

Create alert rules for healthcheck failures.

**File:** `prometheus/rules/healthcheck.yml`

```yaml
groups:
  - name: healthcheck_alerts
    interval: 30s
    rules:
      - alert: HealthcheckDown
        expr: healthcheck_state{state="unhealthy"} == 1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Healthcheck {{ $labels.id }} is down"
          description: "Healthcheck {{ $labels.id }} ({{ $labels.type }}) has been unhealthy for 5 minutes"

      - alert: HighFailureRate
        expr: |
          sum(rate(healthcheck_checks_total{result="failure"}[5m])) by (id, type)
          / sum(rate(healthcheck_checks_total[5m])) by (id, type) > 0.5
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "High failure rate for healthcheck {{ $labels.id }}"
          description: "Healthcheck {{ $labels.id }} has >50% failure rate for 10 minutes"

      - alert: ProxyDisconnected
        expr: healthcheck_proxy_connected == 0
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "Healthcheck server proxy disconnected"
          description: "Healthcheck server has lost connection to seesaw_ha proxy"
```

Mount rules in `docker-compose.yml`:

```yaml
volumes:
  - ./prometheus/rules:/etc/prometheus/rules:ro

command:
  - '--config.file=/etc/prometheus/prometheus.yml'
  - '--storage.tsdb.path=/prometheus'
  - '--web.enable-lifecycle'
```

Update `prometheus.yml`:

```yaml
rule_files:
  - "rules/*.yml"
```

### Custom Metrics Queries

**Success Rate by Type:**
```promql
sum(rate(healthcheck_checks_total{result="success"}[5m])) by (type)
/ sum(rate(healthcheck_checks_total[5m])) by (type)
```

**Average Response Time:**
```promql
sum(rate(healthcheck_response_time_seconds_sum[5m])) by (id, type)
/ sum(rate(healthcheck_response_time_seconds_count[5m])) by (id, type)
```

**State Transitions per Hour:**
```promql
sum(rate(healthcheck_state_transitions_total[1h])) by (from, to)
```

**Batch Efficiency:**
```promql
# Average batch size
sum(rate(healthcheck_batch_size_sum[5m]))
/ sum(rate(healthcheck_batch_size_count[5m]))

# Batches triggered by time vs size
sum(rate(healthcheck_notifications_sent_total[5m])) by (trigger)
```

### Grafana Dashboard Customization

**Add Custom Panel:**
1. Click **Add panel** in dashboard
2. Select visualization type (Graph, Gauge, Stat, etc.)
3. Write PromQL query
4. Configure display options
5. Save panel

**Example Custom Panel - Healthcheck Uptime:**
- Query: `avg_over_time(healthcheck_state{state="healthy"}[24h])`
- Visualization: Stat
- Unit: Percent (0-100)
- Thresholds: Red < 95%, Yellow < 99%, Green >= 99%

## Monitoring Stack Management

### Start/Stop

```bash
# Start
docker-compose up -d

# Stop
docker-compose down

# Stop and remove volumes (DELETES DATA)
docker-compose down -v

# Restart
docker-compose restart
```

### View Logs

```bash
# All services
docker-compose logs -f

# Prometheus only
docker-compose logs -f prometheus

# Grafana only
docker-compose logs -f grafana
```

### Update Configuration

**Prometheus:**
```bash
# Edit prometheus.yml
vim prometheus.yml

# Reload configuration (no restart needed)
curl -X POST http://localhost:9091/-/reload
```

**Grafana:**
```bash
# Restart to pick up provisioning changes
docker-compose restart grafana
```

## Performance Impact

Based on benchmarks (see [PERFORMANCE.md](./PERFORMANCE.md)):

- **Healthcheck Server:** < 0.01% CPU overhead
- **Prometheus Scraping:** ~1-5ms per scrape (every 15s)
- **Network Bandwidth:** < 1 KB/s
- **Memory Overhead:** ~125 KB in healthcheck server, ~200 MB for Prometheus, ~150 MB for Grafana

## References

- **Healthcheck Server Metrics Documentation:** [docs/healthcheck-server-metrics.md](/home/jwillman/projects/seesaw/docs/healthcheck-server-metrics.md)
- **Performance Benchmarks:** [PERFORMANCE.md](./PERFORMANCE.md)
- **Configuration Guide:** [docs/healthcheck-server-config.md](/home/jwillman/projects/seesaw/docs/healthcheck-server-config.md)
- **Prometheus Documentation:** https://prometheus.io/docs/
- **Grafana Documentation:** https://grafana.com/docs/
- **PromQL Guide:** https://prometheus.io/docs/prometheus/latest/querying/basics/

## Support

For issues or questions:
- Check troubleshooting section above
- Review server logs for errors
- Verify configuration files
- Test metrics endpoint directly with curl
- Check Prometheus targets status
