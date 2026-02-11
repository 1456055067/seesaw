# Seesaw v2 Security Hardening Guide

This guide covers how to secure a Seesaw v2 deployment. It describes the security architecture, privilege model, authentication mechanisms, TLS configuration, and network security controls.

## Table of Contents

- [Security Architecture Overview](#security-architecture-overview)
- [Privilege Model](#privilege-model)
- [IPC Authentication and Authorization](#ipc-authentication-and-authorization)
- [TLS Configuration](#tls-configuration)
- [Network Security](#network-security)
- [ECU Authentication](#ecu-authentication)
- [Logging and Auditing](#logging-and-auditing)
- [Operational Security Checklist](#operational-security-checklist)

---

## Security Architecture Overview

Seesaw follows a **least privilege** design with strict trust boundaries:

```
┌─────────────────────────────────────────────────────┐
│                    Seesaw Node                       │
│                                                      │
│  ┌──────────┐    Unix Socket    ┌──────────────────┐│
│  │  CLI     │───────────────────│                  ││
│  │ (any user)│                  │                  ││
│  └──────────┘                   │                  ││
│                                 │     Engine       ││
│  ┌──────────┐    Unix Socket    │  (seesaw user)   ││
│  │  ECU     │───────────────────│                  ││
│  │ (seesaw) │                   │                  ││
│  └──────────┘                   │                  ││
│                                 └────────┬─────────┘│
│  ┌──────────┐    Unix Socket             │          │
│  │  HA      │───────────────────────────►│          │
│  │ (seesaw) │                            │          │
│  └──────────┘                            │          │
│                                          │          │
│  ┌──────────┐    Unix Socket    ┌────────▼─────────┐│
│  │Healthchk │───────────────────│                  ││
│  │ (seesaw) │                   │     NCC          ││
│  └──────────┘                   │   (root)         ││
│                                 │                  ││
│                                 └──────────────────┘│
│                                                      │
│          Mutual TLS (port 10258)                     │
│              ◄─────────────────────────►             │
│                                    Peer Node         │
└─────────────────────────────────────────────────────┘
```

### Key Security Properties

- **All internal IPC** uses Unix domain sockets — not accessible over the network
- **Only NCC runs as root** — all other components run as the unprivileged `seesaw` user
- **Peer sync** uses mutual TLS with certificate-based authentication
- **ECU remote access** is deny-all by default — requires explicit authenticator implementation
- **No hardcoded credentials** — all secrets come from configuration files and certificates

---

## Privilege Model

### Per-Binary Privileges

| Binary | User | Capabilities | Reason |
|--------|------|-------------|--------|
| seesaw_ncc | root | Full | IPVS, iptables, network interfaces, sysctl |
| seesaw_engine | seesaw | None | Drops privileges after initialization |
| seesaw_ha | seesaw | CAP_NET_RAW | Raw IP sockets for VRRPv3 (protocol 112) |
| seesaw_healthcheck | seesaw | CAP_NET_RAW | Raw sockets for ICMP ping checks |
| seesaw_ecu | seesaw | None | HTTP/HTTPS servers only |
| seesaw_watchdog | seesaw | None | Process supervision only |
| seesaw_cli | any | None | Local Unix socket access only |

### Privilege Dropping

The engine starts as root to initialize network resources, then drops to the `seesaw` user. The implementation in `common/server/server.go` (`DropPrivileges`):

1. Looks up the target user via `user.Lookup(username)`
2. Sets GID first via `syscall.Setgid()` — order matters because changing UID first would prevent GID change
3. Sets UID via `syscall.Setuid()`
4. Verifies actual UID and GID match after the drop

### User and Group Setup

Create the seesaw system user:
```bash
useradd --system --no-create-home --shell /usr/sbin/nologin seesaw
```

Set capabilities on binaries that need raw sockets:
```bash
/sbin/setcap cap_net_raw+ep /usr/local/seesaw/seesaw_ha
/sbin/setcap cap_net_raw+ep /usr/local/seesaw/seesaw_healthcheck
```

Set file ownership:
```bash
chown -R seesaw:seesaw /etc/seesaw/
chown -R seesaw:seesaw /var/log/seesaw/
chown -R root:root /var/run/seesaw/   # Created by components at runtime
```

### Development Mode

The `-no_drop_privileges` flag on the engine disables privilege dropping. **Never use this in production.** It is intended only for development and testing.

---

## IPC Authentication and Authorization

### Auth Types

Defined in `common/ipc/ipc.go`:

| Type | Value | Meaning |
|------|-------|---------|
| `ATNone` | 0 | No authentication established |
| `ATSSO` | 1 | Remote user authenticated via ECU |
| `ATTrusted` | 2 | Local trusted component (internal IPC) |
| `ATUntrusted` | 3 | Remote unauthenticated connection |

### Permission Model

```
IsTrusted()   = AuthType == ATTrusted
CanRead()     = IsTrusted() OR (IsAuthenticated() AND IsReader())
CanWrite()    = IsTrusted() OR (IsAuthenticated() AND IsAdmin())

IsAuthenticated() = AuthType == ATSSO
IsAdmin()         = user is member of admin group
IsOperator()      = user is member of admin OR operator group
IsReader()        = user is member of admin OR operator OR reader group
```

### Group-Based Roles

Configure authorization groups at startup:
- `ipc.SetAdminGroup("seesaw-admins")` — full control (read, write, override)
- `ipc.SetOperatorGroup("seesaw-operators")` — operational control
- `ipc.SetReaderGroup("seesaw-readers")` — read-only access

### Per-Vserver Access Control

Each vserver can have `access_grant` entries restricting who can perform overrides:

```protobuf
access_grant: <
  grantee: "dns-oncall"
  role: OPS
  type: GROUP
>
access_grant: <
  grantee: "dns-admin"
  role: ADMIN
  type: GROUP
>
```

Access grant roles:
- `ADMIN` — can perform any operation on the vserver
- `OPS` — can perform operational tasks (enable/disable)

Access grant types:
- `USER` — grantee is a username
- `GROUP` — grantee is a group name (membership defined via `access_groups` in cluster.pb)

### Recommendations

1. Always configure admin, operator, and reader groups
2. Use `access_grant` entries on all vservers to restrict override access to responsible teams
3. Define `access_groups` in cluster.pb for group membership management
4. Implement a custom ECU `Authenticator` if remote access is needed

---

## TLS Configuration

### Certificate Files

Default locations (configurable in EngineConfig):

| File | Default Path | Purpose |
|------|-------------|---------|
| CA Certificate | `/etc/seesaw/ssl/ca.crt` | Trust anchor for peer verification |
| Node Certificate | `/etc/seesaw/ssl/seesaw.crt` | This node's TLS certificate |
| Node Key | `/etc/seesaw/ssl/seesaw.key` | This node's TLS private key |

### Peer Sync (Port 10258) — Mutual TLS

The sync channel between master and backup uses mutual TLS:

- **Protocol:** TLS 1.2 minimum (`tls.VersionTLS12`)
- **Client auth:** `RequireAndVerifyClientCert` — both sides must present valid certificates
- **Certificate validation:** Both certificates must be signed by the cluster CA
- **IP restriction:** The sync server only accepts connections from the configured peer IP or loopback

Configuration in `engine/core.go` (`syncTLSConfig`):
```go
tls.Config{
    Certificates: []tls.Certificate{cert},
    RootCAs:      caCertPool,
    ClientCAs:    caCertPool,
    ClientAuth:   tls.RequireAndVerifyClientCert,
    MinVersion:   tls.VersionTLS12,
}
```

### ECU Control (Port 10256) — Server TLS

The ECU control endpoint uses TLS for the HTTPS server:
- Server certificate and key configured via `ECUCertFile` and `ECUKeyFile`
- CA certificate for client verification via `CACertsFile`
- Configurable `ServerName` for TLS server name indication

### Config Server (Port 10255) — Client TLS

The engine connects to config servers over HTTPS:
- CA certificate pool loaded from `CACertFile`
- Server certificate verification enabled
- HTTP redirects are prohibited (security measure)

### Certificate Management Recommendations

1. **Use a dedicated CA** for the Seesaw cluster — do not reuse production CAs
2. **Set restrictive permissions** on key files:
   ```bash
   chmod 0600 /etc/seesaw/ssl/seesaw.key
   chmod 0644 /etc/seesaw/ssl/seesaw.crt
   chmod 0644 /etc/seesaw/ssl/ca.crt
   chown seesaw:seesaw /etc/seesaw/ssl/*
   ```
3. **Use short-lived certificates** and automate renewal
4. **Include the node hostname** in the certificate's Subject Alternative Names
5. **Rotate certificates** by deploying new certs and restarting the engine

---

## Network Security

### Dynamic Firewall Rules

Seesaw manages its own iptables rules dynamically via the NCC component. Understanding these rules is critical for security hardening.

**Default behavior (applied at initialization):**

```
# Disable connection tracking for all traffic (performance optimization)
iptables -t raw -A PREROUTING -j NOTRACK
iptables -t raw -A OUTPUT -j NOTRACK

# Allow ECU monitoring port
iptables -A INPUT -p tcp --dport 10257 -j ACCEPT

# Default policy: ACCEPT (iptables managed by Seesaw, not host firewall)
```

**Per-VIP rules (applied when a vserver is configured):**

```
# Accept traffic to VIP on configured ports
iptables -A INPUT -d <VIP> -p <proto> --dport <port> -j ACCEPT

# Accept ICMP to VIP
iptables -A INPUT -d <VIP> -p icmp -j ACCEPT

# Reject all other traffic to VIP
iptables -A INPUT -d <VIP> -j REJECT
```

**NAT mode rules (per-service):**

```
# Enable connection tracking for NAT services
iptables -t raw -I PREROUTING -d <VIP> -p <proto> --dport <port> -j CT

# SNAT return traffic with cluster VIP
iptables -t nat -A POSTROUTING -m ipvs --vaddr <VIP> -j SNAT --to-source <ClusterVIP> --random
```

**Firewall mark rules (for FWM mode):**

```
iptables -t mangle -A PREROUTING -d <VIP> -p <proto> --dport <port> -j MARK --set-mark <mark>
```

### Sync Server Access Control

The sync server (`engine/sync.go`) explicitly checks the connecting IP:

```go
// Only accept connections from the configured peer IP or loopback
if !peerIP.Equal(remoteIP) && !remoteIP.IsLoopback() {
    conn.Close()
    continue
}
```

### Host Firewall Recommendations

Since Seesaw manages iptables rules on the LB interface, apply host firewall rules on the **node interface** (eth0):

```bash
# Allow SSH
iptables -A INPUT -i eth0 -p tcp --dport 22 -j ACCEPT

# Allow Seesaw management ports from trusted networks only
iptables -A INPUT -i eth0 -p tcp --dport 10255:10258 -s <trusted_network> -j ACCEPT

# Allow VRRP (protocol 112) from peer only
iptables -A INPUT -i eth0 -p 112 -s <peer_ip> -j ACCEPT

# Allow VRRPv3 multicast
iptables -A INPUT -i eth0 -d 224.0.0.18 -p 112 -j ACCEPT

# Drop everything else on management interface
iptables -A INPUT -i eth0 -j DROP
```

**Do not** add custom iptables rules on the LB interface (eth1) — Seesaw manages those.

### Anycast Range Configuration

The anycast IP ranges determine which VIPs are advertised/withdrawn via BGP. By default, these are `192.168.255.0/24` (IPv4) and `2015:cafe:ffff::/64` (IPv6). For production deployments, override them in the `[anycast_ranges]` section of `seesaw.cfg` to match your network's actual anycast subnet allocation:

```ini
[anycast_ranges]
ipv4 = 10.0.255.0/24
ipv6 = 2001:db8:ffff::/64
```

Omitting one address family disables anycast classification for that family, allowing IPv4-only, IPv6-only, or dual-stack configurations. Ensure the configured ranges match the BGP prefix announcements expected by your upstream routers.

### Sysctl Hardening

Seesaw automatically configures these security-relevant kernel parameters:

**System level:**
| Sysctl | Value | Purpose |
|--------|-------|---------|
| `net.ipv4.ip_forward` | 1 | Required for NAT mode |
| `net.ipv4.vs.conntrack` | 1 | Connection tracking for IPVS |
| `net.ipv4.vs.expire_nodest_conn` | 1 | Remove connections to dead backends |
| `net.netfilter.nf_conntrack_tcp_timeout_established` | 900 | 15-min TCP timeout (vs 5-day default) |

**Per-interface:**
| Sysctl | Value | Purpose |
|--------|-------|---------|
| `arp_filter` | 1 | Only respond to ARP on correct interface |
| `arp_ignore` | 1 | Only respond if address is on receiving interface |
| `arp_announce` | 2 | Use best source address for ARP |
| `rp_filter` | 2 | Loose reverse path filtering |
| `send_redirects` | 0 | Disable ICMP redirects |
| `accept_local` | 1 | Accept traffic with local source (self-hosted VIPs) |
| IPv6 `autoconf` | 0 | Disable SLAAC |
| IPv6 `accept_dad` | 0 | Disable duplicate address detection |

---

## ECU Authentication

### Default Authenticator (Deny-All)

The default `DefaultAuthenticator` in `ecu/auth.go` rejects all authentication attempts:

```go
func (DefaultAuthenticator) Authenticate(ctx *ipc.Context) (*ipc.Context, error) {
    return nil, errors.New("default deny")
}
```

This prevents all remote connections to the ECU control endpoint. **This is the secure default.**

### Implementing Custom Authentication

To enable remote access, implement the `Authenticator` interface:

```go
type Authenticator interface {
    AuthInit() error
    Authenticate(ctx *ipc.Context) (*ipc.Context, error)
}
```

Your `Authenticate` method should:
1. Validate `ctx.AuthToken` against your identity provider
2. Create a new context with `ctx.AuthType = ipc.ATSSO`
3. Populate `ctx.User` with username and group memberships
4. Return the authenticated context

Example patterns:
- **LDAP:** Validate token against LDAP directory, fetch group memberships
- **OAuth:** Validate JWT token, extract claims for user/group info
- **mTLS:** Validate client certificate, extract CN/SAN for identity

### Recommendations

1. If remote management is not needed, keep the `DefaultAuthenticator` — it's the most secure option
2. If remote access is needed, implement authentication with your organization's identity provider
3. Always validate tokens server-side — never trust client-provided identity claims
4. Log all authentication attempts (successful and failed)
5. Consider rate-limiting authentication attempts

---

## Logging and Auditing

### Log Location

All components log to `/var/log/seesaw/`:
- `seesaw_engine.log`, `seesaw_engine.INFO`
- `seesaw_ncc.log`, `seesaw_ncc.INFO`
- `seesaw_ha.log`, `seesaw_ha.INFO`
- `seesaw_healthcheck.log`, `seesaw_healthcheck.INFO`
- `seesaw_ecu.log`, `seesaw_ecu.INFO`
- `seesaw_watchdog.log`, `seesaw_watchdog.INFO`

### Auditable Events

Key events to monitor for security:

| Event | Log Message Pattern | Significance |
|-------|-------------------|--------------|
| HA transition | `"HA state transition"`, `"becomeMaster"`, `"becomeBackup"` | Failover occurred |
| Override request | `"Received override"` | Manual state change |
| Config reload | `"Sending config update notification"` | Configuration changed |
| Sync connection | `"accepted sync connection"` | Peer connected |
| Sync rejection | `"rejecting connection from"` | Unauthorized sync attempt |
| Auth failure | `"authentication failed"` | ECU auth attempt failed |
| Privilege drop | `"Dropping privileges"` | Engine changing user |
| Service crash | `"restarting"` (watchdog) | Component crashed and restarted |

### Recommendations

1. **Configure log rotation** to prevent disk exhaustion:
   ```
   /var/log/seesaw/*.log {
       daily
       rotate 30
       compress
       missingok
       notifempty
   }
   ```
2. **Forward logs** to a centralized logging system (syslog, ELK, etc.)
3. **Alert on** HA transitions, sync rejections, and authentication failures
4. **Set restrictive permissions** on log directories:
   ```bash
   chmod 0750 /var/log/seesaw/
   chown seesaw:seesaw /var/log/seesaw/
   ```

---

## Operational Security Checklist

Use this checklist to verify your Seesaw deployment is properly hardened:

### System Setup
- [ ] Dedicated `seesaw` system user created with no login shell
- [ ] CAP_NET_RAW set **only** on `seesaw_ha` and `seesaw_healthcheck` binaries
- [ ] NCC is the only component running as root
- [ ] Engine drops privileges to `seesaw` user at startup
- [ ] `-no_drop_privileges` flag is **not** used in production

### Certificates and TLS
- [ ] TLS certificates deployed at `/etc/seesaw/ssl/`
- [ ] Private key file permissions set to `0600`
- [ ] Certificates signed by a dedicated Seesaw cluster CA
- [ ] Node hostnames included in certificate SANs
- [ ] Certificate rotation procedure documented and tested

### Authentication and Authorization
- [ ] Admin, operator, and reader groups configured
- [ ] `access_grant` entries configured for all vservers
- [ ] ECU authenticator is either DefaultAuthenticator (deny-all) or a properly implemented custom authenticator
- [ ] No hardcoded credentials in configuration files

### Network
- [ ] Management ports (10255-10258) firewalled from untrusted networks
- [ ] VRRPv3 (protocol 112) restricted to peer IP only
- [ ] LB interface (eth1) managed exclusively by Seesaw — no manual iptables rules
- [ ] Host firewall configured on node interface (eth0)

### Logging
- [ ] Log rotation configured for `/var/log/seesaw/`
- [ ] Logs forwarded to centralized logging system
- [ ] Alerts configured for HA transitions and auth failures
- [ ] Log directory permissions restricted to `seesaw` user

### Configuration
- [ ] cluster.pb does not contain production secrets in plain text
- [ ] Config server communication uses HTTPS with certificate verification
- [ ] RADIUS healthcheck secrets are appropriately protected
- [ ] Config archive directory has restricted permissions
