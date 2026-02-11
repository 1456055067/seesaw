# Seesaw Rust Migration

This directory contains the Rust implementation of performance-critical Seesaw components.

## Project Structure

```
rust/
├── Cargo.toml              # Workspace configuration
├── crates/
│   ├── common/             # Shared error types and utilities
│   ├── ipvs/               # IPVS (IP Virtual Server) bindings ✅
│   ├── ipvs-ffi/           # C FFI bridge for Go interop
│   ├── vrrp/               # VRRPv3 implementation (TODO)
│   └── healthcheck/        # Health check engine (TODO)
└── STATUS.md               # Detailed progress tracking
```

## Completed: IPVS Bindings (Phase 1)

### ✅ Features Implemented

The `ipvs` crate provides pure Rust bindings to the Linux kernel IPVS module via netlink:

**Core Operations:**
- `version()` - Query IPVS kernel version
- `flush()` - Clear all IPVS configuration
- `add_service()` / `update_service()` / `delete_service()` - Virtual service management
- `add_destination()` / `update_destination()` / `delete_destination()` - Backend server management

**Supported Features:**
- IPv4 addresses (IPv6 TODO)
- TCP, UDP, SCTP protocols
- All scheduling algorithms: rr, wrr, lc, wlc, sh, mh
- Firewall mark based services
- All forwarding methods: NAT/Masq, Local, Tunnel, Route/DSR, Bypass
- Service flags and timeouts
- Destination weights and thresholds

### 🎯 Key Achievements

1. **Zero CGo Dependency** - Pure Rust netlink communication
2. **Type Safety** - Strong typing with `Result<T, Error>` throughout
3. **Clean Architecture** - Separated concerns (types, commands, messages, netlink, API)
4. **Comprehensive Tests** - Full integration test suite
5. **~1400 LOC** - Complete implementation in under 1500 lines

### 📊 Progress

- **Phase 1 (IPVS Bindings)**: 85% complete
- **Total Migration**: 28% complete (Phase 1 of 3)

See [STATUS.md](STATUS.md) for detailed progress tracking.

## Building

### Prerequisites

- Rust 1.93+ with Edition 2024
- Linux kernel with IPVS support
- Root privileges for testing

### Build

```bash
cd rust
cargo build --release
```

### Testing

Unit tests (no privileges required):
```bash
cargo test --lib
```

Integration tests (requires root + IPVS module):
```bash
# Load IPVS kernel module
sudo modprobe ip_vs

# Run integration tests
sudo -E IPVS_TEST_ENABLED=1 cargo test --test integration_test -- --nocapture
```

## Performance Goals

Compared to current Go + CGo + libnl implementation:

- **Latency**: 5-10x reduction (eliminate CGo overhead)
- **Throughput**: 2-3x improvement (direct syscalls)
- **Memory**: 50% reduction (no C library overhead)

Benchmarks TODO in Phase 1.6.

## Next Steps

### Phase 1: IPVS Bindings (85% complete)

**Remaining:**
- [ ] Go-Rust FFI bridge (Phase 1.6)
- [ ] Performance benchmarks vs Go implementation
- [ ] Optional: Query operations (get_service/get_services)

### Phase 2: HA VRRP Implementation (0% complete)

Pure Rust VRRPv3 to replace dependencies/ha package:
- VRRPv3 protocol implementation
- Multicast/unicast support
- State machine (INIT/BACKUP/MASTER)
- Priority and preemption handling
- Virtual IP management

### Phase 3: Healthcheck Engine (0% complete)

Rewrite healthchecks in Rust:
- TCP, HTTP, HTTPS, UDP, ICMP checkers
- Concurrent health checking with async/await
- Configurable intervals and timeouts
- State management and callbacks

See [../docs/RUST-MIGRATION-PLAN.md](../docs/RUST-MIGRATION-PLAN.md) for complete migration plan.

## Dependencies

Core dependencies:
- `netlink-packet-core` / `netlink-packet-generic` / `netlink-packet-utils` - Netlink protocol
- `netlink-sys` - Low-level netlink socket
- `tokio` - Async runtime (for future VRRP/healthcheck)
- `nix` - System-level operations
- `tracing` - Structured logging

## Contributing

All commits follow [Conventional Commits](https://www.conventionalcommits.org/) format:

```
feat(ipvs): add new feature
fix(vrrp): fix bug
test(healthcheck): add tests
docs(README): update documentation
```

## License

Apache 2.0 - See [../LICENSE](../LICENSE)

## Resources

- **Migration Plan**: [docs/RUST-MIGRATION-PLAN.md](../docs/RUST-MIGRATION-PLAN.md)
- **Go Reference**: [ipvs/ipvs.go](../ipvs/ipvs.go)
- **Linux IPVS**: `/usr/include/linux/ip_vs.h`
- **Netlink Docs**: https://docs.rs/netlink-packet-core/
