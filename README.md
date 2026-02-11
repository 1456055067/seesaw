# Seesaw v2

[![GoDoc](https://godoc.org/github.com/google/seesaw?status.svg)](https://godoc.org/github.com/google/seesaw)

Note: This is not an official Google product.

## About

Seesaw v2 is a Linux Virtual Server (LVS) based load balancing platform.

It is capable of providing basic load balancing for servers that are on the
same network, through to advanced load balancing functionality such as anycast,
Direct Server Return (DSR), support for multiple VLANs and centralised
configuration.

Above all, it is designed to be reliable and easy to maintain.

## Features

### Why Seesaw?

Seesaw fills a unique niche in the load balancing ecosystem, offering enterprise-grade Layer 4 load balancing with high availability and operational simplicity.

#### Layer 4 Load Balancing at Scale

Unlike Layer 7 proxies (HAProxy, Nginx, Envoy), Seesaw operates at the transport layer:

- **Protocol agnostic** — Load balance TCP, UDP, SCTP, AH, and ESP traffic
- **No protocol parsing** — Minimal CPU overhead, no TLS termination
- **Kernel-level forwarding** — Uses Linux IPVS for line-rate packet forwarding
- **Low latency** — Direct routing modes eliminate proxy overhead

This makes Seesaw ideal for:
- DNS servers
- Database clusters
- Game servers (UDP)
- VPN endpoints
- Any TCP/UDP service requiring high throughput and low latency

#### High Availability Built-In

- **Active/passive clustering** — Two-node setup with automatic failover via VRRPv3
- **Sub-second failover** — Typically <1s for master failure detection
- **Split-brain prevention** — VRRP protocol ensures only one active node
- **Graceful failover** — Manual failover without dropping connections
- **Configuration sync** — Automatic peer synchronization

#### Advanced Load Balancing Modes

**Direct Server Return (DSR)** — Default mode for maximum performance
- Backends respond directly to clients
- Load balancer only handles inbound traffic
- Minimal bandwidth and latency impact on the LB

**NAT Mode** — For backends that can't be configured with VIPs
- Full DNAT/SNAT with connection tracking
- No backend configuration required
- Transparent to backend servers

**IP Tunnel Mode** — For geographically distributed backends
- IP-in-IP encapsulation
- Backends can be in different datacenters

#### Anycast Support

Native BGP integration for global load balancing:

- **Dynamic route advertisement** — Advertise VIPs when healthy, withdraw when unhealthy
- **Quagga BGP integration** — Full-featured BGP speaker
- **Per-vserver control** — Each service can have its own anycast VIP
- **IPv4 and IPv6** — Dual-stack anycast support
- **Configurable ranges** — Define custom anycast IP ranges per deployment

Perfect for:
- Multi-datacenter DNS infrastructure
- Globally distributed services
- DDoS mitigation (traffic directed to nearest healthy site)

#### Rich Scheduling Algorithms

- **Round Robin / Weighted Round Robin** — Simple distribution
- **Least Connections / Weighted Least Connections** — Load-aware routing
- **Source Hashing** — Consistent client-to-backend mapping
- **Maglev Hashing** — Google's consistent hashing algorithm (requires kernel 4.18+)

#### Comprehensive Healthchecks

Beyond simple TCP checks:

- **Multiple protocols** — ICMP ping, TCP, UDP, HTTP/HTTPS, DNS, RADIUS, TCP+TLS
- **DSR/Tunnel mode checks** — Test the full IPVS forwarding path, not just direct connectivity
- **Flexible thresholds** — Configure server watermarks, retries, and timeout behavior
- **TLS verification** — Optional certificate validation for HTTPS/TLS checks
- **Per-port and per-vserver** — Different healthchecks for different service ports

#### Multi-VLAN Support

Serve VIPs across multiple network segments:

- **VLAN sub-interfaces** — eth1.100, eth1.200, etc.
- **Mixed subnet VIPs** — VIPs don't need to be in the LB interface subnet
- **Per-VLAN addressing** — Seesaw can have different IPs on each VLAN

#### Centralized Configuration Management

- **Config server** — Optional HTTPS-based central configuration
- **Automatic reload** — Detects and applies config changes without restart
- **Peer sync** — Backup node automatically syncs from master
- **Fallback sources** — Disk → Peer → Server hierarchy
- **Version tracking** — Configuration change history and metadata
- **Rate limiting** — Protects against mass vserver changes

#### Operational Excellence

- **Protobuf configuration** — Strongly typed, validated config format
- **CLI with tab completion** — Interactive shell for monitoring and control
- **Per-component logging** — Separate logs for engine, HA, healthcheck, NCC, ECU
- **HTTP monitoring API** — JSON stats endpoint for integration with monitoring systems
- **Graceful restarts** — Watchdog-managed component lifecycle
- **Override controls** — Force-enable/disable vservers independent of healthchecks

#### What Seesaw Doesn't Do

To help you choose the right tool:

- **No Layer 7 features** — No HTTP header routing, no TLS termination, no content caching
- **No load balancing algorithms based on application metrics** — Scheduling is connection-based
- **No dynamic backend discovery** — Backends are explicitly configured (no Kubernetes service discovery, etc.)
- **Two-node limit** — Designed for active/passive HA, not horizontal scaling

### Comparison with Other Solutions

| Feature | Seesaw | HAProxy | Nginx | Keepalived + LVS |
|---------|--------|---------|-------|------------------|
| Layer 4 LB | ✓ | ✓ | ✓ | ✓ |
| Layer 7 LB | ✗ | ✓ | ✓ | ✗ |
| DSR Mode | ✓ | ✗ | ✗ | ✓ |
| Anycast BGP | ✓ | ✗ | ✗ | Manual |
| Multi-VLAN | ✓ | ✗ | ✗ | Manual |
| HA Built-in | ✓ | Manual | Manual | ✓ |
| Centralized Config | ✓ | ✗ | ✗ | ✗ |
| Maglev Hashing | ✓ | ✗ | ✗ | ✓ (kernel 4.18+) |
| Protocol Support | TCP/UDP/SCTP/AH/ESP | TCP/UDP (limited) | TCP/UDP (limited) | TCP/UDP/SCTP/AH/ESP |

**Choose Seesaw when you need:**
- Layer 4 load balancing with minimal overhead
- DSR for high-throughput services
- Anycast integration for multi-site deployments
- Operational simplicity with built-in HA
- Non-HTTP protocols (DNS, gaming, VPN, databases)

**Choose HAProxy/Nginx when you need:**
- Layer 7 routing (HTTP path-based routing, header inspection)
- TLS termination
- HTTP/2 or gRPC support
- Content caching
- WebSocket handling

**Choose Keepalived+LVS when you need:**
- Maximum flexibility and control
- Custom scripting and integration
- Non-standard deployment patterns

## Requirements

A Seesaw v2 load balancing cluster requires two Seesaw nodes - these can be
physical machines or virtual instances. Each node must have two network
interfaces - one for the host itself and the other for the cluster VIP. All
four interfaces should be connected to the same layer 2 network.

## Building

Seesaw v2 is developed in Go and depends on several Go packages:

- [golang.org/x/crypto/ssh](http://godoc.org/golang.org/x/crypto/ssh)
- [github.com/dlintw/goconf](http://godoc.org/github.com/dlintw/goconf)
- [github.com/golang/glog](http://godoc.org/github.com/golang/glog)
- [github.com/golang/protobuf/proto](http://godoc.org/github.com/golang/protobuf/proto)
- [github.com/miekg/dns](http://godoc.org/github.com/miekg/dns)

Additionally, there is a compile and runtime dependency on
[libnl](https://www.infradead.org/~tgr/libnl/)

On a Debian/Ubuntu style system, you should be able to prepare for building
by running:

    apt-get install golang
    apt-get install libnl-3-dev libnl-genl-3-dev

If your distro has a go version before 1.18, you may need to fetch a newer
release from https://golang.org/dl/.

If you are running before go version 1.11 or you want to set `GO111MODULE=off`,
after setting `GOPATH` to an appropriate location (for example `~/go`):

    go get -u golang.org/x/crypto/ssh
    go get -u github.com/dlintw/goconf
    go get -u github.com/golang/glog
    go get -u github.com/miekg/dns
    go get -u github.com/kylelemons/godebug/pretty
    go get -u github.com/golang/protobuf/proto

Ensure that `${GOPATH}/bin` is in your `${PATH}` and in the seesaw directory:

    make test
    make install

If you wish to regenerate the protobuf code, the protobuf compiler is needed:

    apt-get install protobuf-compiler

The protobuf code can then be regenerated with:

    make proto

## Installing

After `make install` has run successfully, there should be a number of
binaries in `${GOPATH}/bin` with a `seesaw_` prefix. Install these to the
appropriate locations:

    SEESAW_BIN="/usr/local/seesaw"
    SEESAW_ETC="/etc/seesaw"
    SEESAW_LOG="/var/log/seesaw"

    INIT=`ps -p 1 -o comm=`

    install -d "${SEESAW_BIN}" "${SEESAW_ETC}" "${SEESAW_LOG}"

    install "${GOPATH}/bin/seesaw_cli" /usr/bin/seesaw

    for component in {ecu,engine,ha,healthcheck,ncc,watchdog}; do
      install "${GOPATH}/bin/seesaw_${component}" "${SEESAW_BIN}"
    done

    if [ $INIT = "init" ]; then
      install "etc/init/seesaw_watchdog.conf" "/etc/init"
    elif [ $INIT = "systemd" ]; then
      install "etc/systemd/system/seesaw_watchdog.service" "/etc/systemd/system"
      systemctl --system daemon-reload
    fi
    install "etc/seesaw/watchdog.cfg" "${SEESAW_ETC}"

    # Enable CAP_NET_RAW for seesaw binaries that require raw sockets.
    /sbin/setcap cap_net_raw+ep "${SEESAW_BIN}/seesaw_ha"
    /sbin/setcap cap_net_raw+ep "${SEESAW_BIN}/seesaw_healthcheck"

The `setcap` binary can be found in the libcap2-bin package on Debian/Ubuntu.

## Configuring

Each node needs a `/etc/seesaw/seesaw.cfg` configuration file, which provides
information about the node and who its peer is. Additionally, each load
balancing cluster needs a cluster configuration, which is in the form of a
text-based protobuf - this is stored in `/etc/seesaw/cluster.pb`.

An example seesaw.cfg file can be found in
[etc/seesaw/seesaw.cfg.example](etc/seesaw/seesaw.cfg.example) - a minimal
seesaw.cfg provides the following:

- `anycast_enabled` - True if anycast should be enabled for this cluster.
- `name` - The short name of this cluster.
- `node_ipv4` - The IPv4 address of this Seesaw node.
- `peer_ipv4` - The IPv4 address of our peer Seesaw node.
- `vip_ipv4` - The IPv4 address for this cluster VIP.

The VIP floats between the Seesaw nodes and is only active on the current
master. This address needs to be allocated within the same netblock as both
the node IP address and peer IP address.

An example cluster.pb file can be found in
[etc/seesaw/cluster.pb.example](etc/seesaw/cluster.pb.example) - a minimal
`cluster.pb` contains a `seesaw_vip` entry and two `node` entries. For each
service that you want to load balance, a separate `vserver` entry is
needed, with one or more `vserver_entry` sections (one per port/proto pair),
one or more `backends` and one or more `healthchecks`. Further information
is available in the protobuf definition - see
[pb/config/config.proto](pb/config/config.proto).

On an upstart based system, running `restart seesaw_watchdog` will start (or
restart) the watchdog process, which will in turn start the other components.

### Anycast

Seesaw v2 provides full support for anycast VIPs - that is, it will advertise
an anycast VIP when it becomes available and will withdraw the anycast VIP if
it becomes unavailable. For this to work the Quagga BGP daemon needs to be
installed and configured, with the BGP peers accepting host-specific routes
that are advertised from the Seesaw nodes within the anycast range (currently
hardcoded as `192.168.255.0/24`).

## Command Line

Once initial configuration has been performed and the Seesaw components are
running, the state of the Seesaw can be viewed and controlled via the Seesaw
command line interface. Running `seesaw` (assuming `/usr/bin` is in your path)
will give you an interactive prompt - type `?` for a list of top level
commands. A quick summary:

- `config reload` - reload the cluster.pb from the current config source.
- `failover` - failover between the Seesaw nodes.
- `show vservers` - list all vservers configured on this cluster.
- `show vserver <name>` - show the current state for the named vserver.

## Troubleshooting

A Seesaw should have five components that are running under the watchdog - the
process table should show processes for:

- `seesaw_ecu`
- `seesaw_engine`
- `seesaw_ha`
- `seesaw_healthcheck`
- `seesaw_ncc`
- `seesaw_watchdog`

All Seesaw v2 components have their own logs, in addition to the logging
provided by the watchdog. If any of the processes are not running, check the
corresponding logs in `/var/log/seesaw` (e.g. `seesaw_engine.{log,INFO}`).
