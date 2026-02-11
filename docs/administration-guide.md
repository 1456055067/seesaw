# Seesaw v2 Administration Guide

This guide covers deployment, operations, monitoring, and troubleshooting for Seesaw v2 in production environments.

## Table of Contents

- [Deployment Planning](#deployment-planning)
- [Installation](#installation)
- [Service Management](#service-management)
- [Day-to-Day Operations](#day-to-day-operations)
- [Monitoring](#monitoring)
- [Troubleshooting](#troubleshooting)
- [Backup and Recovery](#backup-and-recovery)
- [Capacity Planning](#capacity-planning)
- [Upgrading](#upgrading)

---

## Deployment Planning

### Hardware / VM Requirements

- **Two nodes** per cluster (physical or virtual)
- **Two network interfaces** per node:
  - **Node interface** (`eth0`) — management, SSH, inter-component communication
  - **LB interface** (`eth1`) — VIP traffic, load balancing, VLAN sub-interfaces
- All four interfaces on the **same Layer 2 network**
- Kernel 4.18+ recommended for Maglev Hash (MH) scheduler

### Network Design

```
┌──────────────────────────────────────────────────────┐
│                  Layer 2 Network                      │
│                                                       │
│  ┌─────────────┐              ┌─────────────┐        │
│  │  Node 1     │              │  Node 2     │        │
│  │ eth0: .2    │              │ eth0: .3    │        │
│  │ eth1: LB    │              │ eth1: LB    │        │
│  └─────────────┘              └─────────────┘        │
│                                                       │
│  Cluster VIP: .1 (floats between nodes)              │
│                                                       │
│  VLANs: eth1.511, eth1.512, ... (for VIP subnets)   │
│                                                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │  Backend 1  │  │  Backend 2  │  │  Backend N  │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  │
└──────────────────────────────────────────────────────┘
```

### IP Address Planning

| Purpose | Example | Notes |
|---------|---------|-------|
| Node 1 IP | 192.168.10.2/24 | Static, on node interface |
| Node 2 IP | 192.168.10.3/24 | Static, on node interface |
| Cluster VIP | 192.168.10.1/24 | Floats, same subnet as node IPs |
| Unicast service VIPs | 192.168.11.x/24 | On VLAN sub-interfaces |
| Anycast service VIPs | 192.168.255.x/24 | Hardcoded range, advertised via BGP |
| Dedicated VIP subnets | 192.168.100.0/26 | Optional, specified in cluster.pb |

Plan IPv6 addresses alongside IPv4 for dual-stack operation.

---

## Installation

### Prerequisites

1. **Kernel modules:**
   ```bash
   modprobe ip_vs
   modprobe nf_conntrack_ipv4
   modprobe dummy numdummies=1
   ```
   Persist via `/etc/modules-load.d/` for systemd.

2. **Build dependencies:**
   ```bash
   apt-get install golang libnl-3-dev libnl-genl-3-dev
   ```

3. **Quagga** (if anycast is needed):
   ```bash
   apt-get install quagga
   ```
   Configure BGP peering in `/etc/quagga/bgpd.conf`.

### Build and Install

```bash
cd seesaw/
make test
make install

SEESAW_BIN="/usr/local/seesaw"
SEESAW_ETC="/etc/seesaw"
install -d "${SEESAW_BIN}" "${SEESAW_ETC}" /var/log/seesaw

install "${GOPATH}/bin/seesaw_cli" /usr/bin/seesaw
for c in ecu engine ha healthcheck ncc watchdog; do
  install "${GOPATH}/bin/seesaw_${c}" "${SEESAW_BIN}"
done

install etc/systemd/system/seesaw_watchdog.service /etc/systemd/system
systemctl daemon-reload

install etc/seesaw/watchdog.cfg "${SEESAW_ETC}"

/sbin/setcap cap_net_raw+ep "${SEESAW_BIN}/seesaw_ha"
/sbin/setcap cap_net_raw+ep "${SEESAW_BIN}/seesaw_healthcheck"
```

### Initial Configuration

**Step 1: Create seesaw.cfg on both nodes**

Node 1 (`/etc/seesaw/seesaw.cfg`):
```ini
[cluster]
anycast_enabled = false
name = au-syd
node_ipv4 = 192.168.10.2
peer_ipv4 = 192.168.10.3
vip_ipv4 = 192.168.10.1

[interface]
node = eth0
lb = eth1
```

Node 2 — same file but swap `node_ipv4` and `peer_ipv4`.

**Step 2: Create cluster.pb**

Start with a minimal config (see [User Guide](user-guide.md#clusterpb--cluster-configuration)), then add vservers.

**Step 3: Deploy SSL certificates**

```bash
mkdir -p /etc/seesaw/ssl
# Deploy ca.crt, seesaw.crt, seesaw.key
chmod 0600 /etc/seesaw/ssl/seesaw.key
chown -R seesaw:seesaw /etc/seesaw/
```

**Step 4: Create seesaw user**

```bash
useradd --system --no-create-home --shell /usr/sbin/nologin seesaw
```

---

## Service Management

### Starting Seesaw

**Systemd:**
```bash
systemctl start seesaw_watchdog
systemctl enable seesaw_watchdog   # Auto-start on boot
```

**Upstart:**
```bash
restart seesaw_watchdog
```

**Manual (development/debugging):**
```bash
seesaw_watchdog -logtostderr
```

### Service Startup Order

The watchdog starts services in dependency order:

```
1. seesaw_ncc         (priority -10, no dependencies)
2. seesaw_engine      (priority -10, depends on ncc, term_timeout 10s)
3. seesaw_ha          (priority -15, depends on engine)
4. seesaw_healthcheck (depends on engine)
5. seesaw_ecu         (depends on engine)
```

### Stopping Seesaw

**Systemd:**
```bash
systemctl stop seesaw_watchdog
```

**Signals:**
| Signal | Behavior |
|--------|----------|
| SIGINT | Graceful shutdown |
| SIGQUIT | Graceful shutdown |
| SIGTERM | Graceful shutdown |
| SIGUSR1 | Dump goroutine stacks to log (debug) |

On graceful shutdown:
1. Engine stops IPC and sync RPC servers
2. Sync client disconnects from peer
3. All vservers are shut down (VIPs removed, IPVS rules deleted)
4. Healthcheck manager stops all checks
5. VLANs are deleted
6. NCC connection is closed

### Watchdog Behavior

- **Restart backoff:** 5 seconds initial, up to 60 seconds maximum
- **Restart delay:** 2 seconds between restart attempts
- **Dependency-aware:** If engine dies, dependent services (HA, healthcheck, ECU) are restarted
- **HA fast failover:** The HA component monitors the engine socket via fsnotify. If the engine crashes and the socket is removed, HA immediately shuts down, causing a VRRPv3 failover

---

## Day-to-Day Operations

### Configuration Changes

**Edit and reload:**
1. Edit `/etc/seesaw/cluster.pb` (or push to config server)
2. Reload via CLI:
   ```
   seesaw> config reload
   ```
3. Verify with:
   ```
   seesaw> show vservers
   seesaw> show warnings
   ```

**Change config source:**
```
seesaw> config source disk       # Use local cluster.pb
seesaw> config source server     # Fetch from HTTPS config servers
seesaw> config source peer       # Fetch from peer node
```

**Automatic reload:** The engine polls for changes every 1 minute.

**Rate limiting:** Max 10 vserver additions or deletions per config cycle. Deletions are processed before additions.

### Failover

**Graceful failover (recommended):**
```
seesaw> failover
```
This causes the current LEADER to:
1. Send a priority-0 VRRPv3 advertisement (shutdown signal)
2. Transition to BACKUP
3. The peer detects priority-0 and immediately becomes LEADER

**Automatic failover:** If the LEADER node fails, the BACKUP promotes itself after `masterDownInterval` (approximately 3 * advertInterval + skewTime).

**Emergency failover:** Kill the HA process on the LEADER:
```bash
pkill seesaw_ha
```
The HA component on the LEADER monitors the engine socket — if the engine dies, HA automatically shuts down, triggering failover.

### Vserver Overrides

Force a vserver to a specific state regardless of healthcheck results:

```
seesaw> override vserver state enabled my-vserver      # Force enable
seesaw> override vserver state disabled my-vserver     # Force disable
seesaw> override vserver state default my-vserver      # Remove override
```

Overrides are:
- Stored in the engine and persist until removed or engine restart
- Synchronized to the peer node
- Require appropriate access (admin role or vserver-specific access_grant)

### Backend Management

Backend state is controlled via `cluster.pb`:

- `status: PRODUCTION` — backend is active
- `status: DISABLED` — backend is not used
- `weight: N` — relative weight for weighted schedulers (default: 1)

Change backend weights by updating cluster.pb and reloading config.

---

## Monitoring

### CLI Monitoring

**Quick status overview:**
```
seesaw> show ha              # HA state (LEADER/BACKUP), transitions
seesaw> show vservers        # All vservers with health status
seesaw> show bgp neighbors   # BGP peer state (if anycast enabled)
seesaw> show nodes           # Cluster nodes (local marked with *)
seesaw> show vlans           # VLAN interface status
```

**Detailed vserver info:**
```
seesaw> show vservers dns.resolver@au-syd
```
Shows services, destinations, weights, healthcheck status, and IPVS statistics (connections, packets, bytes).

**Glob pattern matching:**
```
seesaw> show vservers dns.*    # All vservers matching pattern
```

**Configuration warnings:**
```
seesaw> show warnings
```
Displays misconfigured vservers and other issues detected during config loading.

### ECU Monitoring

| Endpoint | Port | Auth | Purpose |
|----------|------|------|---------|
| Monitor | 10257 | None | Read-only status |
| Control | 10256 | TLS + Auth | Authenticated control |

The ECU collects:
- Cluster status (node health, version, start time)
- Configuration status (last update, source)
- HA status (leader/follower, since timestamp)
- BGP neighbor information
- VLAN information
- Per-vserver state

Stats are collected every 15 seconds and published via the pluggable `Publisher` interface.

### IPVS Statistics

Per-service and per-destination statistics are collected every 15 seconds:
- Active connections
- Inactive connections
- Packets in/out
- Bytes in/out

View via `show vservers <name>` in the CLI.

For raw IPVS statistics:
```bash
ipvsadm -Ln --stats     # On the active node (requires root)
```

### Log Monitoring

Logs are in `/var/log/seesaw/`. Key events to watch for:

| Pattern | Significance |
|---------|-------------|
| `"HA state transition"` | Failover occurred |
| `"VIP ... up"` / `"VIP ... down"` | Vserver state changed |
| `"Sending config update notification"` | New configuration applied |
| `"Failed to"` | Error condition |
| `"Restarting"` (watchdog log) | Component crashed |
| `"rejecting connection from"` | Unauthorized sync attempt |

---

## Troubleshooting

### Component Not Running

**Check process list:**
```bash
ps aux | grep seesaw_
```
Expected: watchdog, ncc, engine, ha, healthcheck, ecu (6 processes).

**Check watchdog log:**
```bash
tail -100 /var/log/seesaw/seesaw_watchdog.log
```
Look for restart loops (repeated "starting" / "died" messages).

**Check dependencies:**
The startup chain is: ncc → engine → {ha, healthcheck, ecu}. If NCC fails, nothing else starts. If engine fails, HA/HC/ECU can't connect.

**Common causes:**
- NCC not running: Check if running as root, check libnl availability
- Engine crash: Check `/var/log/seesaw/seesaw_engine.log` for fatal errors
- Missing kernel modules: `lsmod | grep ip_vs`
- Socket in use: stale `/var/run/seesaw/engine` or `/var/run/seesaw/ncc`

### VIPs Not Working

**Step 1: Check HA state**
```
seesaw> show ha
```
Must show LEADER on the active node. If both show BACKUP, see [HA Problems](#ha-problems).

**Step 2: Check vserver health**
```
seesaw> show vservers <name>
```
Check if the vserver is enabled and healthy. Check individual destination health.

**Step 3: Verify IPVS rules**
```bash
ipvsadm -Ln
```
Verify service entries exist for the VIP:port:protocol combinations. Check destination weights (0 = unhealthy).

**Step 4: Verify iptables**
```bash
iptables -L -n                    # INPUT chain for VIP rules
iptables -t mangle -L -n         # PREROUTING for FWM
iptables -t nat -L -n            # POSTROUTING for NAT
```

**Step 5: Verify VIP on interface**
```bash
ip addr show eth1                 # Or eth1.NNN for VLAN
```

**Step 6: For anycast — check BGP**
```
seesaw> show bgp neighbors
```
Verify peers are Established and routes are advertised.

### Healthcheck Failures

**Check healthcheck logs:**
```bash
tail -200 /var/log/seesaw/seesaw_healthcheck.log
```

**Common causes:**
- Backend down or unreachable
- Firewall blocking healthcheck traffic
- DNS timeout (for DNS healthchecks)
- Certificate verification failure (for HTTPS/TLS checks, set `tls_verify: false` if using self-signed certs)
- Incorrect send/receive strings
- Wrong port number

**DSR/TUN mode healthchecks:**
These route traffic through IPVS. Verify the backend has the VIP on its loopback:
```bash
# On the backend:
ip addr show lo | grep <VIP>
```

**Retry logic:**
A backend is marked unhealthy after `retries + 1` consecutive failures. Check the `retries` field in your healthcheck configuration.

### HA Problems

**Split-brain (both nodes LEADER):**
1. Check VRRPv3 traffic: `tcpdump -i eth0 -n proto 112`
2. Verify multicast delivery to 224.0.0.18
3. Check VRID matches on both nodes (`vrid` in seesaw.cfg)
4. Check network connectivity between nodes

**Both nodes BACKUP:**
1. Check if HA component is running on both nodes
2. Check HA logs for errors
3. Verify engine socket exists (`/var/run/seesaw/engine`)
4. Wait for `masterDownInterval` timeout — one node should promote itself

**Failover not happening:**
1. Check HA timeout (default 30s in engine, masterDownInterval in HA)
2. Verify engine socket is being monitored (fsnotify)
3. Check for VRRPv3 advertisements being received

**Priority conflicts:**
When both nodes have the same priority, the node with the **higher IP address** becomes LEADER (per RFC 5798 section 6.4.3).

### Configuration Issues

**Config not loading:**
```
seesaw> config status
```
Check last update time and current source.

**Config server unreachable:**
Check engine log for "all config server requests failed". The engine falls back to disk after peer fails 3 times.

**Protobuf parse errors:**
Validate `cluster.pb` syntax manually:
```bash
# Check for obvious syntax errors
grep -n "< *$\|> *$" /etc/seesaw/cluster.pb
```

**Misconfigured vservers:**
```
seesaw> show warnings
```
Displays vservers that failed validation during config loading.

### Network Issues

**ARP problems:**
Verify sysctl settings:
```bash
sysctl net.ipv4.conf.eth1.arp_filter    # Should be 1
sysctl net.ipv4.conf.eth1.arp_ignore    # Should be 1
sysctl net.ipv4.conf.eth1.arp_announce  # Should be 2
```

**Routing issues:**
```bash
ip rule show                  # Check policy routing
ip route show table 2         # Check LB routing table
ip route show                 # Check main routing table
```

**VLAN issues:**
```bash
ip link show | grep eth1      # Check VLAN sub-interfaces
ip addr show eth1.511         # Check VIP assignments on VLANs
```

**VMAC issues:**
When using VMAC (`use_vmac = true`), the LB interface MAC should match the VRRP pattern `00:00:5E:00:01:XX` where XX is the VRID in hex:
```bash
ip link show eth1 | grep ether
```

---

## Backup and Recovery

### Files to Backup

| File | Purpose | Frequency |
|------|---------|-----------|
| `/etc/seesaw/seesaw.cfg` | Node configuration | On change |
| `/etc/seesaw/cluster.pb` | Cluster configuration | On change |
| `/etc/seesaw/ssl/*` | TLS certificates and keys | On rotation |
| `/etc/seesaw/watchdog.cfg` | Service startup config | On change |

### Recovery Procedure

1. Install Seesaw binaries (same version)
2. Restore configuration files from backup
3. Create seesaw user if not present
4. Set capabilities on HA and healthcheck binaries
5. Start the watchdog: `systemctl start seesaw_watchdog`
6. The node will join the cluster as BACKUP
7. Config will sync from the LEADER automatically

### Peer Sync

The backup node automatically syncs configuration and healthcheck state from the master. If both nodes are lost, restore from backup files.

---

## Capacity Planning

### IPVS Limits

| Resource | Limit | Notes |
|----------|-------|-------|
| Firewall marks | 8000 | Allocated from base 256 |
| DSR healthcheck marks | 16000 | Allocated from base 16384 |
| IPVS services | Kernel memory | Practical limit: thousands |
| IPVS destinations per service | Kernel memory | Practical limit: thousands |

### Healthcheck Scaling

- Each healthcheck runs as a goroutine
- Results are batched (max 100 per notification)
- Config updates are batched from the engine
- Practical limit: thousands of concurrent healthchecks

### Configuration Rate Limiting

- Max 10 vserver additions per config cycle
- Max 10 vserver deletions per config cycle
- Config cycle: every 1 minute (or on manual reload)
- Large config changes may take multiple cycles to fully apply

### Sync Limits

- Sync buffer: 100 notes per session
- Poll timeout: 30 seconds
- Heartbeat interval: 5 seconds
- Session deadtime: 2 minutes

### File Descriptor Limits

The systemd service file sets `LimitNOFILE=8192` to support many concurrent healthcheck connections. Increase this if running many healthchecks:

```ini
# /etc/systemd/system/seesaw_watchdog.service
[Service]
LimitNOFILE=16384
```

---

## Upgrading

### Rolling Upgrade Procedure

1. **Verify current state:**
   ```
   seesaw> show ha          # Identify LEADER and BACKUP
   seesaw> show vservers    # Verify all healthy
   ```

2. **Upgrade the BACKUP node first:**
   ```bash
   # On BACKUP node:
   systemctl stop seesaw_watchdog
   # Install new binaries
   /sbin/setcap cap_net_raw+ep /usr/local/seesaw/seesaw_ha
   /sbin/setcap cap_net_raw+ep /usr/local/seesaw/seesaw_healthcheck
   systemctl start seesaw_watchdog
   ```

3. **Verify BACKUP is running new version:**
   ```
   seesaw> show version
   ```

4. **Failover to the upgraded BACKUP:**
   ```
   seesaw> failover
   ```

5. **Verify the new LEADER is healthy:**
   ```
   seesaw> show ha
   seesaw> show vservers
   ```

6. **Upgrade the old LEADER (now BACKUP):**
   Repeat step 2 on the other node.

7. **Optionally failover back** to the original LEADER:
   ```
   seesaw> failover
   ```

### Configuration Migration

If the new version changes the protobuf schema:
1. Update `cluster.pb` on the config server first
2. Both old and new versions should handle unknown fields gracefully (protobuf forward compatibility)
3. After both nodes are upgraded, update local `cluster.pb` files

### Rollback

If issues are found after upgrade:
1. Failover away from the upgraded node: `seesaw> failover`
2. Stop the upgraded node: `systemctl stop seesaw_watchdog`
3. Reinstall the previous version
4. Restart: `systemctl start seesaw_watchdog`
