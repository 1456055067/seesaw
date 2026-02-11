# Seesaw v2 User Guide

This guide covers installation, configuration, and day-to-day use of Seesaw v2.

## Table of Contents

- [Introduction](#introduction)
- [Requirements](#requirements)
- [Building and Installing](#building-and-installing)
- [Configuration Files](#configuration-files)
- [Load Balancing Modes](#load-balancing-modes)
- [Scheduling Algorithms](#scheduling-algorithms)
- [VIP Types](#vip-types)
- [Configuring Vservers](#configuring-vservers)
- [Configuring Healthchecks](#configuring-healthchecks)
- [VLAN Configuration](#vlan-configuration)
- [High Availability](#high-availability)
- [CLI Reference](#cli-reference)
- [Monitoring via ECU](#monitoring-via-ecu)
- [Configuration Sources and Reload](#configuration-sources-and-reload)

---

## Introduction

Seesaw v2 is a Linux Virtual Server (LVS) based Layer 4 load balancer. Unlike Layer 7 load balancers such as HAProxy or Nginx, Seesaw operates at the transport layer. This means:

- It can load balance **TCP, UDP, SCTP, AH, and ESP** traffic
- It does **not** perform HTTP header inspection, TLS termination, or content-based routing
- It uses the Linux kernel IPVS module for packet forwarding, providing high performance

Seesaw runs as a two-node active/passive cluster with automatic failover via VRRPv3.

---

## Requirements

### Hardware / VM

- **Two nodes** (physical or virtual) for high availability
- **Two network interfaces** per node:
  - `eth0` (node interface) — management traffic, SSH
  - `eth1` (LB interface) — VIP traffic, load balancing
- All four interfaces must be on the **same Layer 2 network**

### Kernel Modules

The following kernel modules must be loaded before starting Seesaw:

- `ip_vs` — IP Virtual Server module for kernel-level load balancing
- `nf_conntrack_ipv4` — connection tracking for NAT mode
- `dummy` — virtual network interface for dummy0

Load them manually:
```bash
modprobe ip_vs
modprobe nf_conntrack_ipv4
modprobe dummy numdummies=1
```

Or persist via systemd:
```bash
echo ip_vs > /etc/modules-load.d/ip_vs.conf
echo nf_conntrack_ipv4 > /etc/modules-load.d/nf_conntrack_ipv4.conf
echo "dummy numdummies=1" > /etc/modules-load.d/dummy.conf
systemctl restart systemd-modules-load.service
```

### Build Dependencies

- Go 1.18+
- libnl-3-dev, libnl-genl-3-dev (for IPVS netlink bindings)
- protobuf-compiler (only if regenerating protobuf code)

```bash
apt-get install golang libnl-3-dev libnl-genl-3-dev
```

---

## Building and Installing

### Build

```bash
cd seesaw/
make test      # Run tests
make install   # Build and install binaries to ${GOPATH}/bin
```

### Install

```bash
SEESAW_BIN="/usr/local/seesaw"
SEESAW_ETC="/etc/seesaw"
SEESAW_LOG="/var/log/seesaw"

install -d "${SEESAW_BIN}" "${SEESAW_ETC}" "${SEESAW_LOG}"

# Install CLI to /usr/bin
install "${GOPATH}/bin/seesaw_cli" /usr/bin/seesaw

# Install daemons
for component in ecu engine ha healthcheck ncc watchdog; do
  install "${GOPATH}/bin/seesaw_${component}" "${SEESAW_BIN}"
done

# Install service file (systemd)
install etc/systemd/system/seesaw_watchdog.service /etc/systemd/system
systemctl daemon-reload

# Install watchdog config
install etc/seesaw/watchdog.cfg "${SEESAW_ETC}"

# Set capabilities for raw sockets
/sbin/setcap cap_net_raw+ep "${SEESAW_BIN}/seesaw_ha"
/sbin/setcap cap_net_raw+ep "${SEESAW_BIN}/seesaw_healthcheck"
```

---

## Configuration Files

### seesaw.cfg — Node Configuration

**Path:** `/etc/seesaw/seesaw.cfg`
**Format:** INI

Each node has its own `seesaw.cfg`. This configures the node identity and cluster membership.

```ini
[cluster]
anycast_enabled = false
name = au-syd
node_ipv4 = 192.168.10.2
node_ipv6 = 2015:cafe::2
peer_ipv4 = 192.168.10.3
peer_ipv6 = 2015:cafe::3
vip_ipv4 = 192.168.10.1
vip_ipv6 = 2015:cafe::1

# Anycast network ranges (optional).
# Omit a line to disable that address family.
# For IPv4-only: set only ipv4
# For IPv6-only: set only ipv6
# For dual-stack: set both
[anycast_ranges]
# ipv4 = 10.0.255.0/24
# ipv6 = 2001:db8:ffff::/64

[config_server]
primary = seesaw-config1.example.com
secondary = seesaw-config2.example.com
tertiary = seesaw-config3.example.com

[interface]
node = eth0
lb = eth1
```

**All options with defaults:**

| Option | Default | Description |
|--------|---------|-------------|
| `anycast_enabled` | `true` | Enable/disable anycast VIP support |
| `name` | (required) | Short name of this cluster |
| `node_ipv4` / `node_ipv6` | (required) | This node's IP address |
| `peer_ipv4` / `peer_ipv6` | (required) | Peer node's IP address |
| `vip_ipv4` / `vip_ipv6` | (required) | Cluster VIP (floats between nodes) |
| `vrid` | `60` | VRRP virtual router ID (1-255) |
| `use_vmac` | `true` | Use VRRP MAC (false = use gratuitous ARP) |
| `garp_interval_sec` | `10` | Gratuitous ARP interval in seconds |
| `config_server` primary/secondary/tertiary | `seesaw-config.example.com` | Config server hostnames |
| `node` interface | `eth0` | Management network interface |
| `lb` interface | `eth1` | Load balancing network interface |
| `anycast_ranges` ipv4 | `192.168.255.0/24` | IPv4 anycast CIDR range (omit to disable IPv4 anycast) |
| `anycast_ranges` ipv6 | `2015:cafe:ffff::/64` | IPv6 anycast CIDR range (omit to disable IPv6 anycast) |

### cluster.pb — Cluster Configuration

**Path:** `/etc/seesaw/cluster.pb`
**Format:** Text protobuf

This is the cluster-wide configuration containing VIPs, nodes, vservers, backends, healthchecks, and VLANs. See [pb/config/config.proto](../pb/config/config.proto) for the full schema.

Minimal example:
```protobuf
seesaw_vip: <
  fqdn: "seesaw-vip1.example.com."
  ipv4: "192.168.10.1/24"
  ipv6: "2015:cafe::1/64"
  status: PRODUCTION
>
node: <
  fqdn: "seesaw1-1.example.com."
  ipv4: "192.168.10.2/24"
  ipv6: "2015:cafe::2/64"
  status: PRODUCTION
>
node: <
  fqdn: "seesaw1-2.example.com."
  ipv4: "192.168.10.3/24"
  ipv6: "2015:cafe::3/64"
  status: PRODUCTION
>
```

See [etc/seesaw/cluster.pb.example](../etc/seesaw/cluster.pb.example) for a full example with vservers, BGP peers, VLANs, and access grants.

### watchdog.cfg — Service Startup

**Path:** `/etc/seesaw/watchdog.cfg`
**Format:** INI

Defines which services the watchdog manages, their dependencies, and startup order. The default configuration is installed from `etc/seesaw/watchdog.cfg`.

---

## Load Balancing Modes

### DSR (Direct Server Return)

**Default mode.** Also known as "gatewaying" in ipvsadm terms.

- The load balancer rewrites the destination MAC to the backend's MAC
- The backend responds **directly to the client** (not through the load balancer)
- **Requirement:** Backends must have the VIP configured on their loopback interface
- **Best for:** Most use cases, highest performance, minimal load balancer overhead

### NAT (Network Address Translation)

Also known as "masquerading" in ipvsadm terms.

- Full DNAT/SNAT — all return traffic passes through the load balancer
- The load balancer rewrites source IP to the cluster VIP
- **No backend configuration** required
- **Best for:** When backends cannot have VIP on loopback, or when inspecting return traffic
- **Note:** Connection tracking is enabled per-NAT-service automatically

### Tunnel (IP-in-IP)

Also known as "ipip" in ipvsadm terms.

- Packets are encapsulated in an IP-in-IP tunnel to the backend
- Backends must support IP-in-IP decapsulation
- **Best for:** Geographically distributed backends

---

## Scheduling Algorithms

Set via the `scheduler` field in `VserverEntry`:

| Scheduler | Name | Description |
|-----------|------|-------------|
| `RR` | Round Robin | Equal distribution across all backends |
| `WRR` | Weighted Round Robin | Distribution proportional to backend weights |
| `LC` | Least Connections | Routes to backend with fewest active connections |
| `WLC` | Weighted Least Connections | LC weighted by backend weight **(default)** |
| `SH` | Source Hashing | Consistent hash based on source IP |
| `MH` | Maglev Hashing | Google's Maglev consistent hash (kernel 4.18+) |

**Notes on MH (Maglev):**
- Requires kernel 4.18 or later
- Typically used without conntrack sync
- May require `net.ipv4.vs.sloppy_tcp` sysctl for seamless failover
- Disabling conntrack sync could affect other services using other schedulers

---

## VIP Types

### Unicast VIPs

Standard VIPs assigned to the LB interface or a VLAN sub-interface. VIPs in the same subnet as the LB interface are configured on that interface; VIPs in other subnets use VLAN sub-interfaces.

### Anycast VIPs

VIPs advertised via BGP for anycast routing. When a vserver with an anycast VIP becomes healthy, the route is advertised; when it becomes unhealthy, the route is withdrawn.

Requirements:
- `anycast_enabled = true` in seesaw.cfg
- Quagga BGP daemon installed and configured
- BGP peers configured in cluster.pb to accept host-specific routes
- VIP address must fall within the configured anycast range

The anycast ranges default to `192.168.255.0/24` (IPv4) and `2015:cafe:ffff::/64` (IPv6). Override these via the `[anycast_ranges]` section in seesaw.cfg to match your network's actual anycast subnets:

```ini
# IPv4-only anycast
[anycast_ranges]
ipv4 = 10.0.255.0/24

# IPv6-only anycast
[anycast_ranges]
ipv6 = 2001:db8:ffff::/64

# Dual-stack anycast
[anycast_ranges]
ipv4 = 10.0.255.0/24
ipv6 = 2001:db8:ffff::/64
```

Omitting a line disables that address family for anycast classification. Any VIP not in the configured anycast range(s) will be treated as a unicast or dedicated VIP.

---

## Configuring Vservers

A vserver represents a load-balanced service. Each vserver has:

- **entry_address** — the VIP hostname and IP(s)
- **vserver_entry** — one or more port/protocol combinations with scheduling and health checks
- **backend** — one or more backend servers
- **healthcheck** — vserver-level health checks (apply to all entries)

### Example: DNS Load Balancer

```protobuf
vserver: <
  name: "dns.resolver@au-syd"
  entry_address: <
    fqdn: "dns-anycast.example.com."
    ipv4: "192.168.255.1/24"
    status: PRODUCTION
  >
  rp: "dns-team@example.com"
  vserver_entry: <
    protocol: UDP
    port: 53
    scheduler: RR
    server_low_watermark: 0.3
    healthcheck: <
      type: DNS
      interval: 5
      timeout: 2
      port: 53
      send: "www.example.com"
      receive: "192.168.0.1"
      mode: DSR
      method: "a"
      retries: 1
    >
  >
  vserver_entry: <
    protocol: TCP
    port: 53
    scheduler: RR
    server_low_watermark: 0.3
    healthcheck: <
      type: DNS
      interval: 5
      timeout: 2
      port: 53
      send: "www.example.com"
      receive: "192.168.0.1"
      mode: DSR
      method: "a"
      retries: 1
    >
  >
  backend: <
    host: <
      fqdn: "dns1-1.example.com."
      ipv4: "192.168.12.1/24"
      status: PRODUCTION
    >
    weight: 1
  >
  backend: <
    host: <
      fqdn: "dns1-2.example.com."
      ipv4: "192.168.12.2/24"
      status: PRODUCTION
    >
    weight: 1
  >
>
```

### VserverEntry Fields

| Field | Default | Description |
|-------|---------|-------------|
| `protocol` | (required) | TCP or UDP |
| `port` | (required) | Service port number |
| `scheduler` | WLC | Scheduling algorithm |
| `mode` | DSR | Load balancing mode (DSR, NAT, TUN) |
| `persistence` | 0 (disabled) | Session persistence timeout in seconds |
| `quiescent` | false | Continue routing to unhealthy backends for existing connections |
| `server_low_watermark` | 0.0 | Min healthy fraction to stay active |
| `server_high_watermark` | 0.0 | Min healthy fraction to become active |
| `lthreshold` | 0 | IPVS lower connection threshold |
| `uthreshold` | 0 | IPVS upper connection threshold |
| `one_packet` | false | One-packet scheduling (UDP) |
| `healthcheck` | (none) | Per-entry health checks |

### Firewall Mark Mode

Set `use_fwm: true` on the vserver to use a single firewall mark for all entries instead of individual per-port/protocol IPVS services. This is useful when multiple ports need to share the same persistence group.

### Watermarks

- **server_low_watermark** — if healthy backends drop below this fraction, the vserver becomes unhealthy
- **server_high_watermark** — healthy backends must reach this fraction for the vserver to become healthy (hysteresis)
- If `server_low_watermark` is unset, `server_high_watermark` is used for both

---

## Configuring Healthchecks

Healthchecks can be defined at the vserver level (apply to all entries) or per vserver_entry.

### Common Fields

| Field | Default | Description |
|-------|---------|-------------|
| `type` | (required) | ICMP_PING, TCP, UDP, HTTP, HTTPS, DNS, TCP_TLS, RADIUS |
| `interval` | 10 | Check interval in seconds |
| `timeout` | 5 | Check timeout in seconds |
| `port` | (entry port) | Port to check (required for vserver-level checks) |
| `mode` | PLAIN | Check mode: PLAIN, DSR, TUN |
| `retries` | 0 | Consecutive failures before marking unhealthy |
| `tls_verify` | true | Verify TLS certificates |

### TCP Healthcheck

```protobuf
healthcheck: <
  type: TCP
  port: 8080
  send: "PING\r\n"
  receive: "PONG"
  tls_verify: false
>
```

### HTTP / HTTPS Healthcheck

```protobuf
healthcheck: <
  type: HTTP
  port: 80
  send: "/healthz"
  receive: "OK"
  code: 200
  method: "GET"
  proxy: false
  tls_verify: false
>
```

- `send` — URL path (for HTTP) or full URL (for proxy mode)
- `receive` — expected substring in response body
- `code` — expected HTTP status code
- `method` — HTTP method (default: GET)
- `proxy` — send request as proxy request (full URL in request line)
- Use `type: HTTPS` for HTTPS checks (equivalent to `type: HTTP` with TLS enabled)

### DNS Healthcheck

```protobuf
healthcheck: <
  type: DNS
  port: 53
  send: "www.example.com"
  receive: "192.168.0.1"
  method: "a"
>
```

- `send` — DNS query name
- `receive` — expected answer
- `method` — query type: "a", "aaaa", "cname", "ns", "soa", etc.

### ICMP Ping Healthcheck

```protobuf
healthcheck: <
  type: ICMP_PING
  port: 0
>
```

The simplest healthcheck. Note: ICMP ping does not support DSR or TUN modes.

### UDP Healthcheck

```protobuf
healthcheck: <
  type: UDP
  port: 9999
  send: "PING"
  receive: "PONG"
>
```

### RADIUS Healthcheck

```protobuf
healthcheck: <
  type: RADIUS
  port: 1812
  radius_username: "monitor"
  radius_password: "ignored"
  radius_secret: "sharedsecret"
  radius_response: "accept"
  mode: DSR
>
```

For backward compatibility, the legacy format `send: "username:password:secret"` and `receive: "accept"` is also supported.

### DSR and TUN Mode Healthchecks

When using `mode: DSR` or `mode: TUN`, the healthcheck daemon sends traffic through the IPVS infrastructure (using a dedicated firewall mark) rather than connecting directly to the backend. This tests the full data path including kernel IPVS forwarding.

---

## VLAN Configuration

VLANs allow VIPs to be on different subnets from the LB interface. Each VLAN creates a sub-interface (e.g., `eth1.511`).

```protobuf
vlan: <
  vlan_id: 511
  host: <
    fqdn: "seesaw1-vlan511.example.com."
    ipv4: "192.168.11.252/24"
    ipv6: "2015:cafe:11::ff01/64"
  >
>
```

The `host` entry specifies the IP address Seesaw uses on that VLAN. VIPs within the VLAN's subnet will be assigned to the corresponding sub-interface.

---

## High Availability

Seesaw uses VRRPv3 (RFC 5798) for high availability between two nodes.

### How It Works

1. Both nodes start as BACKUP
2. After `masterDownInterval` without receiving master advertisements, a node becomes LEADER
3. The LEADER sends periodic VRRPv3 advertisements (multicast to 224.0.0.18)
4. If the LEADER fails, the BACKUP takes over after the timeout
5. Priority determines which node becomes LEADER (higher = preferred)
6. If priorities are equal, the node with the higher IP address wins

### VMAC vs Gratuitous ARP

Two failover modes (controlled by `use_vmac` in seesaw.cfg):

- **VMAC (default)** — uses a virtual MAC address (00:00:5E:00:01:XX where XX is the VRID). No ARP updates needed on failover since MAC stays the same.
- **Gratuitous ARP** — sends GARP messages to update network switches after failover. Required for IPv4-only; IPv6 support not yet available in this mode.

### Graceful Failover

From the CLI:
```
seesaw> failover
```

This causes the current LEADER to send a priority-0 advertisement (graceful shutdown signal) and transition to BACKUP. The peer node will detect the priority-0 advertisement and immediately become LEADER.

---

## CLI Reference

Start the CLI:
```bash
seesaw    # or /usr/bin/seesaw
```

### Commands

| Command | Description |
|---------|-------------|
| `config reload` | Reload cluster.pb from current config source |
| `config source` | View current config source |
| `config source {disk\|server\|peer}` | Change config source |
| `config status` | Show config status and metadata |
| `failover` | Trigger graceful failover to peer node |
| `show bgp neighbors` | Display BGP peer status and statistics |
| `show backends` | List all backends across all vservers |
| `show destinations` | List all destinations |
| `show ha` | Show HA state, transitions, sent/received counts |
| `show nodes` | List cluster nodes (local node marked with `*`) |
| `show version` | Show Seesaw engine version |
| `show vlans` | List configured VLANs |
| `show vservers` | List all vservers with status |
| `show vservers <name>` | Detailed view of a specific vserver (supports glob patterns) |
| `show warnings` | Show configuration warnings |
| `override vserver state enabled <name>` | Force-enable a vserver |
| `override vserver state disabled <name>` | Force-disable a vserver |
| `override vserver state default <name>` | Remove override, return to healthcheck-driven state |
| `help` or `?` | Show available commands |
| `exit` or `quit` | Exit CLI |

Commands support prefix matching (e.g., `sh v` for `show vservers`).

---

## Monitoring via ECU

The External Control Unit (ECU) provides remote monitoring and control:

### Monitoring Endpoint (Port 10257)

HTTP endpoint for read-only monitoring. No authentication required.

Collects and publishes:
- Cluster status (node health, version, uptime)
- Configuration status (last update, source)
- HA status (leader/follower, since timestamp)
- BGP neighbor info
- VLAN information
- Vserver state (per-vserver details)

### Control Endpoint (Port 10256)

HTTPS endpoint for authenticated control operations. Requires a custom `Authenticator` implementation (the default denies all connections).

### Statistics

Stats are collected every 15 seconds (configurable via `StatsInterval`) and published via a pluggable `Publisher` interface for integration with external monitoring systems.

---

## Configuration Sources and Reload

### Source Priority

At bootstrap, configuration sources are tried in order:
1. **Peer** — fetch from peer node via sync RPC
2. **Disk** — read local `/etc/seesaw/cluster.pb`
3. **Server** — fetch from configured HTTPS config servers (port 10255)

Once running:
- **Master** node uses `SourceServer` by default
- **Backup** node syncs configuration from the master

### Automatic Reload

The engine checks for config changes every 1 minute (configurable via `ConfigInterval`). If the config has changed, it applies the update.

### Manual Reload

```
seesaw> config reload
```

### Changing Config Source

```
seesaw> config source disk      # Use local cluster.pb
seesaw> config source server    # Fetch from config servers
seesaw> config source peer      # Fetch from peer node
```

### Rate Limiting

Configuration updates are rate-limited to a maximum of 10 vserver additions or deletions per config cycle. This prevents large config changes from overwhelming the system. Deletions are processed before additions to avoid conflicts.

### Peer Sync Fallback

If fetching config from the peer fails `MaxPeerConfigSyncErrors` times (default: 3), the engine falls back to fetching from the config server.
