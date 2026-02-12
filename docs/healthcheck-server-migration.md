# Healthcheck Server Configuration Migration Guide

## Overview

This guide helps existing healthcheck-server deployments adopt YAML-based configuration. The configuration system is **100% backward compatible** - no changes are required to keep using the server with default settings.

## Migration Status

**Current Version**: Configuration support added in Phase 5

**Backward Compatibility**: ✅ Complete
- Existing deployments continue working without changes
- Configuration is entirely optional
- Defaults match previous hardcoded behavior exactly

## Do I Need to Migrate?

**No action required** if:
- Current default settings work for your deployment
- You don't need to tune performance parameters
- Default socket path (`/var/run/seesaw/healthcheck-proxy.sock`) is correct

**Consider migrating** if you want to:
- Tune batching behavior for your workload
- Adjust buffer sizes for high-volume deployments
- Change polling intervals for responsiveness/efficiency
- Enable debug logging without environment variables
- Use a non-standard socket path
- Document your configuration in version control

## Migration Approaches

### Approach 1: No Migration (Recommended for Most)

**When**: Default settings are adequate

**Action**: None

**Result**: Server continues using built-in defaults

**Risk**: None

---

### Approach 2: Gradual Migration (Recommended for Production)

**When**: Want to customize some settings while maintaining safety

**Steps**:

1. **Review current behavior** - Server is using these defaults:
   ```yaml
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
     format: "text"
   ```

2. **Create minimal config** - Start with minimal overrides:
   ```yaml
   # /etc/seesaw/healthcheck-server.yaml
   server:
     proxy_socket: "/var/run/seesaw/healthcheck-proxy.sock"

   logging:
     level: "info"
     format: "json"  # Enable JSON logging
   ```

3. **Test in staging** - Deploy to staging environment first

4. **Validate configuration**:
   ```bash
   # Check for successful load message
   journalctl -u healthcheck-server | grep "Configuration loaded"
   # Expected: "Configuration loaded successfully"
   ```

5. **Monitor behavior** - Verify metrics and logs show expected behavior

6. **Deploy to production** - Roll out to production with monitoring

7. **Iterate** - Gradually add tuning parameters as needed

**Risk**: Low (minimal changes, tested in stages)

---

### Approach 3: Full Configuration (For Custom Deployments)

**When**: Need extensive customization or want explicit documentation

**Steps**:

1. **Choose example config** - Select from provided examples:
   - `healthcheck-server-production.yaml` (standard production)
   - `healthcheck-server-high-volume.yaml` (many healthchecks)
   - `healthcheck-server-development.yaml` (development/testing)

2. **Copy and customize**:
   ```bash
   # Copy example
   sudo cp rust/crates/healthcheck-server/examples/healthcheck-server-production.yaml \
           /etc/seesaw/healthcheck-server.yaml

   # Customize as needed
   sudo vim /etc/seesaw/healthcheck-server.yaml
   ```

3. **Validate syntax** - Use a YAML validator:
   ```bash
   # Using yamllint (if available)
   yamllint /etc/seesaw/healthcheck-server.yaml
   ```

4. **Test startup**:
   ```bash
   # Restart server
   sudo systemctl restart healthcheck-server

   # Check for errors
   sudo systemctl status healthcheck-server
   journalctl -u healthcheck-server -n 50
   ```

5. **Verify configuration loaded**:
   ```bash
   # Should see "Configuration loaded successfully"
   journalctl -u healthcheck-server | grep -i config
   ```

6. **Monitor performance** - Watch for any unexpected behavior changes

**Risk**: Medium (extensive changes require careful testing)

## Configuration File Placement

**Recommended locations by deployment type:**

| Deployment Type | Recommended Location | Rationale |
|----------------|---------------------|-----------|
| Production | `/etc/seesaw/healthcheck-server.yaml` | System-wide, survives updates |
| Development | `./healthcheck-server.yaml` | Per-project, easy to modify |
| User-specific | `~/.config/seesaw/healthcheck-server.yaml` | User testing |
| Container | `/config/healthcheck-server.yaml` | Mounted volume |

**Search Priority**: `/etc/seesaw` → `~/.config/seesaw` → `./`

## Common Migration Scenarios

### Scenario 1: Enable JSON Logging

**Goal**: Enable structured logging for log aggregation

**Before**: Using environment variable
```bash
# systemd service override
Environment="RUST_LOG=info"
```

**After**: Using configuration
```yaml
# /etc/seesaw/healthcheck-server.yaml
logging:
  level: "info"
  format: "json"
```

**Migration**:
```bash
# Create config file
sudo mkdir -p /etc/seesaw
sudo tee /etc/seesaw/healthcheck-server.yaml > /dev/null <<EOF
logging:
  level: "info"
  format: "json"
EOF

# Restart server
sudo systemctl restart healthcheck-server

# Verify JSON format
journalctl -u healthcheck-server -n 10
```

---

### Scenario 2: Tune for High Volume

**Goal**: Handle 1000+ healthchecks efficiently

**Before**: Using defaults (may experience backpressure)

**After**: Using high-volume configuration
```yaml
# /etc/seesaw/healthcheck-server.yaml
batching:
  delay: 250ms
  max_size: 1000

channels:
  notification: 10000
  config_update: 100
  proxy_message: 100

logging:
  level: "info"
  format: "json"
```

**Migration**:
```bash
# Use provided example
sudo cp rust/crates/healthcheck-server/examples/healthcheck-server-high-volume.yaml \
        /etc/seesaw/healthcheck-server.yaml

# Restart and monitor
sudo systemctl restart healthcheck-server
journalctl -u healthcheck-server -f
```

**Monitoring**: Watch for reduced notification latency and no channel overflow warnings

---

### Scenario 3: Development Environment

**Goal**: Fast feedback during development

**Before**: Production settings (slow for development)

**After**: Development-optimized configuration
```yaml
# ./healthcheck-server.yaml (local directory)
server:
  proxy_socket: "/tmp/healthcheck-proxy.sock"

batching:
  delay: 50ms
  max_size: 10

manager:
  monitor_interval: 100ms

logging:
  level: "debug"
  format: "text"
```

**Migration**:
```bash
# Copy example to project directory
cp rust/crates/healthcheck-server/examples/healthcheck-server-development.yaml \
   ./healthcheck-server.yaml

# Run server (will find local config)
cargo run -p healthcheck-server
```

## Validation Before Deployment

**Pre-deployment checklist**:

1. **Syntax validation**:
   ```bash
   # Visual inspection
   cat /etc/seesaw/healthcheck-server.yaml

   # YAML validation (if yamllint available)
   yamllint /etc/seesaw/healthcheck-server.yaml
   ```

2. **Dry run** (optional): Test in non-production environment first

3. **Backup current config** (if modifying existing):
   ```bash
   sudo cp /etc/seesaw/healthcheck-server.yaml \
           /etc/seesaw/healthcheck-server.yaml.backup
   ```

4. **Test server startup**:
   ```bash
   # Check for validation errors
   sudo systemctl restart healthcheck-server
   sudo systemctl status healthcheck-server
   ```

5. **Verify log messages**:
   ```bash
   # Should see "Configuration loaded successfully"
   journalctl -u healthcheck-server -n 20 | grep -i config
   ```

## Rollback Procedure

If configuration causes issues:

**Quick Rollback** (remove config):
```bash
# Remove config file
sudo rm /etc/seesaw/healthcheck-server.yaml

# Restart server (will use defaults)
sudo systemctl restart healthcheck-server
```

**Restore Backup**:
```bash
# Restore previous config
sudo cp /etc/seesaw/healthcheck-server.yaml.backup \
        /etc/seesaw/healthcheck-server.yaml

# Restart server
sudo systemctl restart healthcheck-server
```

**Emergency** (if server won't start):
```bash
# Remove config and restart
sudo rm /etc/seesaw/healthcheck-server.yaml
sudo systemctl restart healthcheck-server

# Check status
sudo systemctl status healthcheck-server
```

## Performance Tuning Recommendations

### Low-Volume Deployments (< 100 healthchecks)

**Use defaults** - No configuration needed

**Optional optimizations**:
```yaml
manager:
  monitor_interval: 1s  # Reduce CPU usage
```

---

### Medium-Volume Deployments (100-500 healthchecks)

**Use production example** as baseline:
```yaml
batching:
  delay: 100ms
  max_size: 100

channels:
  notification: 1000
```

---

### High-Volume Deployments (500+ healthchecks)

**Use high-volume example**:
```yaml
batching:
  delay: 250ms
  max_size: 1000

channels:
  notification: 10000
  config_update: 100
  proxy_message: 100
```

**Monitor**: Watch for channel overflow warnings or high memory usage

---

### Latency-Sensitive Deployments

**Minimize batching delay**:
```yaml
batching:
  delay: 50ms
  max_size: 50

manager:
  monitor_interval: 250ms
```

**Trade-off**: Higher CPU usage, more frequent proxy communication

## Monitoring Configuration Changes

**What to monitor after migration**:

1. **Startup logs** - "Configuration loaded successfully"
2. **Error rate** - No increase in errors
3. **Latency** - Notification delivery time
4. **Memory usage** - Channel buffer memory
5. **CPU usage** - Impact of polling interval changes

**Key log messages**:
```
# Success
"Configuration loaded successfully"

# Fallback to defaults
"No configuration file found, using defaults"
"Configuration error: ... Using default configuration"
```

**Metrics to watch**:
- Notification batch size (should match config)
- Notification latency (should match batching delay)
- Channel depth (should not approach buffer size)
- CPU usage (affected by monitor_interval)

## Troubleshooting

### Problem: Configuration not loading

**Symptoms**: Logs show "No configuration file found"

**Solutions**:
1. Verify file exists: `ls -la /etc/seesaw/healthcheck-server.yaml`
2. Check permissions: File must be readable by healthcheck-server user
3. Verify filename: Must be exactly `healthcheck-server.yaml` (not `.yml`)

---

### Problem: Validation errors

**Symptoms**: "Validation error" at startup, server uses defaults

**Solutions**:
1. Read error message - indicates which field and constraint violated
2. Check value ranges in [Configuration Reference](healthcheck-server-config.md)
3. Verify duration format: `100ms` not `100`
4. Compare against example configs

---

### Problem: Server behavior unchanged

**Symptoms**: Configuration file exists but behavior seems unchanged

**Solutions**:
1. Verify "Configuration loaded successfully" in logs
2. Check you're editing the right file (search priority order)
3. Restart server after config changes
4. Verify values are actually different from defaults

---

### Problem: Performance degradation

**Symptoms**: Higher latency or CPU usage after config change

**Solutions**:
1. Review changed values - identify likely culprits
2. Try production example as baseline
3. Rollback to defaults and iterate carefully
4. Monitor metrics while adjusting one parameter at a time

## Version Control

**Recommended**: Store configuration in version control

**Example**:
```bash
# Add to repository
git add /etc/seesaw/healthcheck-server.yaml
git commit -m "feat: add healthcheck-server configuration"

# Deploy with configuration management (Ansible, etc.)
- name: Deploy healthcheck-server config
  copy:
    src: files/healthcheck-server.yaml
    dest: /etc/seesaw/healthcheck-server.yaml
    owner: root
    group: root
    mode: '0644'
  notify: restart healthcheck-server
```

## Summary

**Key Points**:
- ✅ Configuration is optional - defaults work for most deployments
- ✅ 100% backward compatible - existing deployments unchanged
- ✅ Gradual migration recommended - start minimal, iterate
- ✅ Easy rollback - remove config file to restore defaults
- ✅ Comprehensive validation - helpful error messages
- ✅ Example configs provided - for common scenarios

**Next Steps**:
1. Review [Configuration Reference](healthcheck-server-config.md)
2. Choose migration approach (none, gradual, or full)
3. Test in non-production environment
4. Deploy with monitoring
5. Iterate based on metrics

## See Also

- [Configuration Reference](healthcheck-server-config.md) - Complete configuration documentation
- [Deployment Guide](HEALTHCHECK_HYBRID_DEPLOYMENT.md) - Full deployment instructions
- Example configurations in `rust/crates/healthcheck-server/examples/`
