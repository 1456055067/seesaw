# Seesaw Rust Performance Optimization Plan

## Executive Summary

This document outlines the implementation plan for migrating performance-critical components of Seesaw from Go to Rust. The goal is to eliminate GC-induced latency spikes, reduce CGo overhead, and improve deterministic timing for high-availability operations.

**Target Components:**
1. Netlink/IPVS interface (~1,200 LOC)
2. HA VRRP packet processing (~600 LOC)
3. Healthcheck engine (~2,000 LOC)

**Expected Benefits:**
- 5-10x faster IPVS updates (eliminate CGo overhead)
- Sub-millisecond VRRP timing jitter (eliminate GC pauses)
- 2-5x healthcheck throughput
- 30-50% memory reduction for concurrent operations

---

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Phase 1: Netlink/IPVS Bindings](#phase-1-netlinkipvs-bindings)
- [Phase 2: HA VRRP Implementation](#phase-2-ha-vrrp-implementation)
- [Phase 3: Healthcheck Engine](#phase-3-healthcheck-engine)
- [Integration Strategy](#integration-strategy)
- [Testing Strategy](#testing-strategy)
- [Rollout Plan](#rollout-plan)
- [Risk Mitigation](#risk-mitigation)
- [Success Metrics](#success-metrics)

---

## Architecture Overview

### Current Architecture (Go)

```
┌─────────────────────────────────────────────────────────────┐
│                    Go Process Space                          │
│                                                              │
│  ┌──────────┐    ┌──────────┐    ┌──────────────┐         │
│  │  Engine  │───▶│   NCC    │───▶│ Network Mgmt │         │
│  │(Orchestr)│    │          │    │   (netlink)  │         │
│  └─────┬────┘    └──────────┘    └───────┬──────┘         │
│        │                                   │                 │
│        │         ┌──────────┐              │                 │
│        ├────────▶│    HA    │              │                 │
│        │         │  (VRRP)  │              │                 │
│        │         └──────────┘              │                 │
│        │                                   │                 │
│        │         ┌──────────┐              │                 │
│        └────────▶│HealthChk │              │                 │
│                  └──────────┘              │                 │
│                                            │                 │
│                  ┌──────────┐              │                 │
│                  │   IPVS   │◀─────────────┘                 │
│                  │ (libnl)  │ CGo calls                      │
│                  └────┬─────┘                                │
└───────────────────────┼──────────────────────────────────────┘
                        │ netlink
                        ▼
              ┌─────────────────┐
              │  Linux Kernel   │
              │   IPVS Module   │
              └─────────────────┘
```

**Pain Points:**
- CGo overhead in IPVS operations (10-100x slower than native calls)
- GC pauses affecting VRRP timing (10-50ms spikes)
- Memory allocations in hot paths
- Unpredictable latency for time-critical operations

### Target Architecture (Hybrid Go/Rust)

```
┌─────────────────────────────────────────────────────────────┐
│                    Go Process Space                          │
│                                                              │
│  ┌──────────┐    ┌──────────┐                              │
│  │  Engine  │───▶│   NCC    │  (Keep in Go)                │
│  │(Orchestr)│    │          │                              │
│  └─────┬────┘    └──────────┘                              │
│        │ gRPC/FFI                                           │
└────────┼────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│                   Rust Process Space                         │
│                                                              │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐ │
│  │  IPVS Manager│    │   HA VRRP    │    │ Healthcheck  │ │
│  │  (netlink)   │    │ (raw sockets)│    │   Engine     │ │
│  └──────┬───────┘    └──────────────┘    └──────────────┘ │
│         │                                                   │
└─────────┼───────────────────────────────────────────────────┘
          │ netlink syscalls (no CGo)
          ▼
    ┌─────────────────┐
    │  Linux Kernel   │
    │   IPVS Module   │
    └─────────────────┘
```

**Benefits:**
- Direct netlink syscalls (no CGo)
- No GC pauses in critical paths
- Zero-copy packet handling
- Predictable, deterministic latency

---

## Phase 1: Netlink/IPVS Bindings

**Goal:** Replace Go+CGo netlink/IPVS implementation with pure Rust netlink syscalls.

### 1.1 Setup Rust Environment

**Tasks:**
- [ ] Initialize Rust workspace in `rust/` directory
- [ ] Configure Cargo.toml with required dependencies
- [ ] Set up CI/CD for Rust compilation
- [ ] Add cross-compilation support (if needed)

**Dependencies:**
```toml
[dependencies]
netlink-packet-core = "0.7"
netlink-packet-generic = "0.3"
netlink-sys = "0.8"
nix = { version = "0.28", features = ["socket", "net"] }
libc = "0.2"
thiserror = "1.0"
tracing = "0.1"
tokio = { version = "1", features = ["full"] }
```

**Deliverable:** `rust/Cargo.toml` with workspace configuration

### 1.2 Implement IPVS Data Types

**Current Go types to port:**
- `ipvs.Service`
- `ipvs.Destination`
- `ipvs.ServiceFlags`
- `ipvs.DestinationFlags`
- `ipvs.ServiceStats` / `ipvs.DestinationStats`

**Tasks:**
- [ ] Create `rust/crates/ipvs/src/types.rs`
- [ ] Implement IPVS structs with proper netlink attributes
- [ ] Add serialization/deserialization for netlink messages
- [ ] Implement Display/Debug traits for all types
- [ ] Add unit tests for type conversions

**Reference Files:**
- Go: `ipvs/ipvs.go` lines 48-335
- Kernel headers: `/usr/include/linux/ip_vs.h`

**Example Rust structure:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    pub address: IpAddr,
    pub protocol: Protocol,
    pub port: u16,
    pub fwmark: u32,
    pub scheduler: Scheduler,
    pub flags: ServiceFlags,
    pub timeout: u32,
    pub persistence_engine: Option<String>,
    pub statistics: ServiceStats,
}

impl Service {
    pub fn to_netlink(&self) -> NetlinkMessage { /* ... */ }
    pub fn from_netlink(msg: &NetlinkMessage) -> Result<Self> { /* ... */ }
}
```

**Deliverable:** `rust/crates/ipvs/src/types.rs` with complete type definitions

### 1.3 Implement Netlink Communication Layer

**Tasks:**
- [ ] Create netlink socket wrapper
- [ ] Implement generic netlink family lookup (`IPVS` family)
- [ ] Create message builder for IPVS commands
- [ ] Implement message parser for responses
- [ ] Add error handling and retry logic
- [ ] Add connection pooling (if beneficial)

**IPVS Netlink Commands to Implement:**
```rust
pub enum IPVSCommand {
    NewService,      // IPVS_CMD_NEW_SERVICE
    SetService,      // IPVS_CMD_SET_SERVICE
    DelService,      // IPVS_CMD_DEL_SERVICE
    GetService,      // IPVS_CMD_GET_SERVICE
    NewDest,         // IPVS_CMD_NEW_DEST
    SetDest,         // IPVS_CMD_SET_DEST
    DelDest,         // IPVS_CMD_DEL_DEST
    GetDest,         // IPVS_CMD_GET_DEST
    Flush,           // IPVS_CMD_FLUSH
    GetInfo,         // IPVS_CMD_GET_INFO
}
```

**Deliverable:** `rust/crates/ipvs/src/netlink.rs` with netlink communication

### 1.4 Implement IPVS Operations API

**Tasks:**
- [ ] Implement `ipvs::init()` - Initialize IPVS family
- [ ] Implement `ipvs::flush()` - Flush all services
- [ ] Implement `ipvs::add_service()` - Add service
- [ ] Implement `ipvs::update_service()` - Update service
- [ ] Implement `ipvs::delete_service()` - Delete service
- [ ] Implement `ipvs::get_service()` - Get specific service
- [ ] Implement `ipvs::get_services()` - List all services
- [ ] Implement `ipvs::add_destination()` - Add destination to service
- [ ] Implement `ipvs::update_destination()` - Update destination
- [ ] Implement `ipvs::delete_destination()` - Delete destination
- [ ] Add comprehensive error types

**API Design:**
```rust
pub struct IPVSManager {
    family: u16,
    socket: NetlinkSocket,
}

impl IPVSManager {
    pub fn new() -> Result<Self, IPVSError>;
    pub fn version(&self) -> Result<IPVSVersion, IPVSError>;
    pub fn flush(&mut self) -> Result<(), IPVSError>;
    pub fn add_service(&mut self, svc: &Service) -> Result<(), IPVSError>;
    pub fn update_service(&mut self, svc: &Service) -> Result<(), IPVSError>;
    pub fn delete_service(&mut self, svc: &Service) -> Result<(), IPVSError>;
    pub fn get_services(&self) -> Result<Vec<Service>, IPVSError>;
    pub fn add_destination(&mut self, svc: &Service, dst: &Destination) -> Result<(), IPVSError>;
    // ... etc
}
```

**Deliverable:** `rust/crates/ipvs/src/lib.rs` with complete public API

### 1.5 Integration Testing

**Tasks:**
- [ ] Create integration test suite using actual IPVS kernel module
- [ ] Test all CRUD operations (Create, Read, Update, Delete)
- [ ] Test error conditions (invalid IPs, missing services, etc.)
- [ ] Performance benchmarks vs Go+CGo implementation
- [ ] Memory leak tests (long-running operations)
- [ ] Concurrent operation tests

**Test Infrastructure:**
```rust
#[cfg(test)]
mod integration_tests {
    #[test]
    fn test_service_lifecycle() { /* ... */ }

    #[test]
    fn test_destination_crud() { /* ... */ }

    #[test]
    fn test_concurrent_updates() { /* ... */ }
}
```

**Deliverable:** `rust/crates/ipvs/tests/integration.rs`

### 1.6 Go-Rust Bridge

**Tasks:**
- [ ] Create C-compatible FFI interface
- [ ] Implement CGo wrappers for Rust functions
- [ ] Create Go package that calls Rust via CGo
- [ ] Add error propagation from Rust to Go
- [ ] Benchmark FFI overhead

**FFI Interface:**
```rust
// rust/crates/ipvs-ffi/src/lib.rs
#[no_mangle]
pub extern "C" fn ipvs_init() -> *mut IPVSManager { /* ... */ }

#[no_mangle]
pub extern "C" fn ipvs_add_service(
    manager: *mut IPVSManager,
    svc_json: *const c_char,
) -> i32 { /* ... */ }
```

**Go Wrapper:**
```go
// ipvs/rust_bridge.go
package ipvs

/*
#cgo LDFLAGS: -L./rust/target/release -lipvs_ffi
#include "./rust/crates/ipvs-ffi/include/ipvs.h"
*/
import "C"

func Init() error {
    mgr := C.ipvs_init()
    // ...
}
```

**Deliverable:** `rust/crates/ipvs-ffi/` with FFI bindings

### Phase 1 Success Criteria

- [ ] All IPVS operations functional via Rust
- [ ] 5-10x performance improvement vs Go+CGo
- [ ] Zero crashes in 72-hour soak test
- [ ] Feature parity with Go implementation
- [ ] Documentation complete

**Estimated Timeline:** 4-6 weeks

---

## Phase 2: HA VRRP Implementation

**Goal:** Implement VRRP v3 packet handling in Rust for deterministic failover timing.

### 2.1 VRRP Packet Structures

**Tasks:**
- [ ] Create `rust/crates/vrrp/src/packet.rs`
- [ ] Implement VRRPv3 advertisement struct (RFC 5798)
- [ ] Implement checksum calculation (IPv4 pseudo-header, IPv6 pseudo-header)
- [ ] Add packet serialization/deserialization
- [ ] Add packet validation

**Reference:**
- Go implementation: `ha/net.go` lines 40-54
- RFC 5798: VRRPv3 specification

**Rust Structure:**
```rust
#[repr(C, packed)]
pub struct VRRPAdvertisement {
    pub version_type: u8,    // Version (4 bits) + Type (4 bits)
    pub vrid: u8,            // Virtual Router ID
    pub priority: u8,        // Priority
    pub count_ip_addrs: u8,  // Count of IP addresses
    pub advert_int: u16,     // Advertisement interval (centiseconds)
    pub checksum: u16,       // VRRP checksum
}

impl VRRPAdvertisement {
    pub fn new(vrid: u8, priority: u8, advert_int: Duration) -> Self;
    pub fn compute_checksum_v4(&mut self, src: Ipv4Addr, dst: Ipv4Addr);
    pub fn compute_checksum_v6(&mut self, src: Ipv6Addr, dst: Ipv6Addr);
    pub fn validate(&self) -> Result<(), VRRPError>;
    pub fn to_bytes(&self) -> [u8; 8];
    pub fn from_bytes(buf: &[u8]) -> Result<Self, VRRPError>;
}
```

**Deliverable:** `rust/crates/vrrp/src/packet.rs`

### 2.2 Raw Socket Implementation

**Tasks:**
- [ ] Create raw IP socket (protocol 112 = VRRP)
- [ ] Implement socket options (TTL=255, multicast settings)
- [ ] Create async send/receive operations
- [ ] Handle IPv4 and IPv6 addressing
- [ ] Implement timeout and deadline handling

**Socket Configuration:**
```rust
pub struct VRRPSocket {
    socket: Socket,
    local_addr: IpAddr,
    remote_addr: IpAddr,
}

impl VRRPSocket {
    pub async fn new(local: IpAddr, remote: IpAddr) -> Result<Self>;
    pub async fn send(&self, advert: &VRRPAdvertisement, timeout: Duration) -> Result<()>;
    pub async fn recv(&self) -> Result<VRRPAdvertisement>;
    pub async fn recv_with_timeout(&self, timeout: Duration) -> Result<VRRPAdvertisement>;
}
```

**Deliverable:** `rust/crates/vrrp/src/socket.rs`

### 2.3 VRRP State Machine

**Tasks:**
- [ ] Implement VRRP states (INIT, BACKUP, LEADER)
- [ ] Implement state transitions per RFC 5798
- [ ] Implement master_down_interval calculation
- [ ] Implement priority handling
- [ ] Add preemption logic
- [ ] Implement graceful shutdown (priority 0 advertisement)

**State Machine:**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VRRPState {
    Init,
    Backup,
    Leader,
}

pub struct VRRPNode {
    config: VRRPConfig,
    state: VRRPState,
    socket: VRRPSocket,
    master_down_interval: Duration,
    last_advert_time: Instant,
}

impl VRRPNode {
    pub async fn run(&mut self) -> Result<()>;
    async fn run_backup(&mut self) -> Result<()>;
    async fn run_leader(&mut self) -> Result<()>;
    fn transition_to(&mut self, new_state: VRRPState);
    fn calculate_master_down_interval(&self) -> Duration;
}
```

**Deliverable:** `rust/crates/vrrp/src/state_machine.rs`

### 2.4 Integration with Go Engine

**Tasks:**
- [ ] Create gRPC service for VRRP state reporting
- [ ] Implement status updates to Go engine
- [ ] Add configuration hot-reload
- [ ] Implement graceful failover trigger from CLI
- [ ] Add telemetry and logging

**gRPC Service:**
```rust
// rust/crates/vrrp/proto/vrrp.proto
service VRRPService {
    rpc GetStatus(StatusRequest) returns (StatusResponse);
    rpc TriggerFailover(FailoverRequest) returns (FailoverResponse);
    rpc UpdateConfig(ConfigRequest) returns (ConfigResponse);
}
```

**Deliverable:** `rust/crates/vrrp/src/grpc_server.rs`

### 2.5 Testing and Validation

**Tasks:**
- [ ] Unit tests for packet handling
- [ ] Unit tests for checksum calculation
- [ ] Integration tests with two VRRP instances
- [ ] Failover timing tests (measure sub-ms precision)
- [ ] Priority preemption tests
- [ ] Stress tests (long-running stability)
- [ ] Compare timing with Go implementation

**Test Scenarios:**
- Normal operation (BACKUP receiving advertisements)
- Master failure detection
- Master recovery (backup stepping down)
- Split-brain prevention
- Priority-based preemption
- Graceful failover

**Deliverable:** `rust/crates/vrrp/tests/` test suite

### Phase 2 Success Criteria

- [ ] VRRP fully functional in Rust
- [ ] <1ms jitter in advertisement timing
- [ ] Failover detection <500ms consistently
- [ ] No false failovers under load
- [ ] Compatible with Go-based engine
- [ ] 48-hour soak test passed

**Estimated Timeline:** 3-4 weeks

---

## Phase 3: Healthcheck Engine

**Goal:** Implement high-throughput async healthcheck engine in Rust.

### 3.1 Healthcheck Protocol Implementations

**Tasks:**
- [ ] Implement ICMP ping checker
- [ ] Implement TCP checker (with optional TLS)
- [ ] Implement UDP checker
- [ ] Implement HTTP/HTTPS checker
- [ ] Implement DNS checker
- [ ] Implement RADIUS checker
- [ ] Add DSR/TUN mode support (via IPVS forwarding)

**Checker Trait:**
```rust
#[async_trait]
pub trait HealthChecker: Send + Sync {
    async fn check(&self, target: &Backend) -> HealthCheckResult;
    fn check_type(&self) -> CheckType;
}

pub struct TCPChecker {
    port: u16,
    send: Option<String>,
    receive: Option<String>,
    tls_verify: bool,
}

#[async_trait]
impl HealthChecker for TCPChecker {
    async fn check(&self, target: &Backend) -> HealthCheckResult {
        // Tokio-based async TCP connection
    }
}
```

**Deliverable:** `rust/crates/healthcheck/src/checkers/`

### 3.2 Async Scheduler

**Tasks:**
- [ ] Create Tokio-based task scheduler
- [ ] Implement configurable check intervals
- [ ] Add jitter to avoid thundering herd
- [ ] Implement backoff on repeated failures
- [ ] Add per-backend concurrency limits
- [ ] Implement result batching for efficiency

**Scheduler Design:**
```rust
pub struct HealthCheckScheduler {
    backends: HashMap<BackendId, BackendState>,
    checkers: HashMap<CheckType, Box<dyn HealthChecker>>,
    results_tx: mpsc::Sender<HealthCheckResult>,
}

impl HealthCheckScheduler {
    pub async fn run(&mut self) -> Result<()>;
    async fn schedule_backend(&self, backend: &Backend);
    async fn execute_check(&self, backend: &Backend, checker: &dyn HealthChecker);
}
```

**Deliverable:** `rust/crates/healthcheck/src/scheduler.rs`

### 3.3 Result Aggregation and Notification

**Tasks:**
- [ ] Implement result batching (reduce RPC overhead)
- [ ] Add hysteresis (avoid flapping)
- [ ] Implement retry logic with backoff
- [ ] Create gRPC service for sending results to engine
- [ ] Add telemetry (Prometheus metrics)

**Result Batching:**
```rust
pub struct ResultBatcher {
    batch: Vec<HealthCheckResult>,
    batch_size: usize,
    batch_delay: Duration,
    tx: mpsc::Sender<Vec<HealthCheckResult>>,
}

impl ResultBatcher {
    pub async fn submit(&mut self, result: HealthCheckResult);
    async fn flush(&mut self);
}
```

**Deliverable:** `rust/crates/healthcheck/src/aggregator.rs`

### 3.4 gRPC Integration with Engine

**Tasks:**
- [ ] Define healthcheck protocol buffers
- [ ] Implement gRPC server for receiving check configs
- [ ] Implement gRPC client for sending results
- [ ] Add bidirectional streaming for efficiency
- [ ] Implement reconnection logic

**Protocol:**
```protobuf
// rust/crates/healthcheck/proto/healthcheck.proto
service HealthCheckService {
    rpc StreamChecks(stream CheckConfig) returns (stream CheckResult);
    rpc UpdateConfig(ConfigUpdate) returns (ConfigUpdateResponse);
}
```

**Deliverable:** `rust/crates/healthcheck/src/grpc.rs`

### 3.5 Performance Optimization

**Tasks:**
- [ ] Implement connection pooling for TCP/HTTP checks
- [ ] Add DNS caching
- [ ] Optimize memory allocations (use object pools)
- [ ] Add SIMD optimizations where applicable
- [ ] Profile and optimize hot paths

**Deliverable:** Optimized implementation with benchmarks

### 3.6 Testing and Benchmarking

**Tasks:**
- [ ] Unit tests for each checker type
- [ ] Integration tests with mock backends
- [ ] Load tests (10,000+ backends)
- [ ] Latency profiling
- [ ] Memory usage profiling
- [ ] Compare performance with Go implementation

**Benchmark Targets:**
- 2-5x throughput improvement
- <10% CPU at 10K backends
- <500MB memory at 10K backends

**Deliverable:** `rust/crates/healthcheck/benches/` benchmark suite

### Phase 3 Success Criteria

- [ ] All healthcheck types implemented
- [ ] 2-5x throughput vs Go implementation
- [ ] Scales to 10,000+ backends
- [ ] <1% false positives in 24-hour test
- [ ] Memory usage <50% of Go version
- [ ] Full integration with engine

**Estimated Timeline:** 6-8 weeks

---

## Integration Strategy

### Communication Between Go and Rust

**Option 1: gRPC (Recommended)**

**Pros:**
- Language-agnostic
- Bi-directional streaming
- Well-defined interface (protobuf)
- Battle-tested at scale
- Easy to debug with grpcurl/grpcui

**Cons:**
- Slight overhead vs FFI
- Requires separate processes

**Architecture:**
```
┌─────────────┐           ┌──────────────┐
│ Go Engine   │  gRPC     │ Rust IPVS    │
│             │◀─────────▶│   Manager    │
│             │           └──────────────┘
│             │
│             │  gRPC     ┌──────────────┐
│             │◀─────────▶│ Rust VRRP    │
│             │           │   Node       │
│             │           └──────────────┘
│             │
│             │  gRPC     ┌──────────────┐
│             │◀─────────▶│ Rust Health  │
│             │           │   Checker    │
└─────────────┘           └──────────────┘
```

**Option 2: FFI with CGo**

**Pros:**
- Lowest latency
- Single process
- Simpler deployment

**Cons:**
- Complex error handling
- Rust must expose C-compatible API
- CGo overhead still exists (but less than netlink CGo)
- Harder to debug

**Recommendation:** Start with gRPC for clean separation, consider FFI later if latency is critical.

### Process Management

**Option 1: Separate Rust Processes (Recommended)**
- Each Rust component runs as separate process
- Watchdog manages all processes (Go + Rust)
- Easier to restart individual components
- Better isolation

**Option 2: Single Rust Process**
- All Rust components in one binary
- Single gRPC server with multiple services
- Simpler deployment
- Shared dependencies

**Recommendation:** Separate processes for flexibility.

### Configuration Management

**Approach:**
1. Go engine reads and validates cluster.pb
2. Engine sends relevant config to Rust services via gRPC
3. Rust services apply config and return status
4. Config reload triggers updates to all Rust services

### Logging and Observability

**Strategy:**
- Rust components use `tracing` crate
- Export logs via gRPC to Go engine
- Or write to separate log files
- Add Prometheus metrics endpoint in each Rust service

---

## Testing Strategy

### Unit Testing

**Go Components:**
- Existing tests remain unchanged
- Add tests for gRPC client integration

**Rust Components:**
- Comprehensive unit tests for all modules
- Use `cargo test` with coverage tracking
- Target >80% code coverage

### Integration Testing

**Test Scenarios:**
1. Full cluster deployment (2 nodes, Go + Rust)
2. IPVS operations under load
3. HA failover scenarios
4. Healthcheck accuracy
5. Configuration reload
6. Component restart/recovery

**Test Environment:**
- Docker-based test cluster
- Automated with GitHub Actions
- Use `testcontainers-rs` for isolation

### Performance Testing

**Benchmarks:**
1. IPVS operation latency (add/update/delete service)
2. VRRP advertisement timing precision
3. Healthcheck throughput (checks/sec)
4. Memory usage under load
5. CPU utilization

**Tools:**
- `criterion.rs` for Rust benchmarks
- `pprof` for Go profiling
- Flamegraphs for bottleneck identification

### Regression Testing

**Strategy:**
- Run existing Seesaw test suite
- Ensure no functionality regressions
- Compare behavior with pure-Go implementation
- Long-running soak tests (7+ days)

---

## Rollout Plan

### Stage 1: Development and Testing (Weeks 1-12)

- Implement Phase 1 (IPVS)
- Implement Phase 2 (VRRP)
- Implement Phase 3 (Healthcheck)
- Complete integration testing

### Stage 2: Internal Deployment (Weeks 13-16)

- Deploy to dev/staging clusters
- Run parallel with Go implementation
- Compare metrics and behavior
- Fix bugs and optimize

### Stage 3: Canary Rollout (Weeks 17-20)

- Deploy to 1% of production clusters
- Monitor for issues
- Gradual rollout: 1% → 5% → 25% → 50%
- Rollback plan if issues detected

### Stage 4: Full Production (Weeks 21-24)

- Complete rollout to all clusters
- Retire Go implementation (optional)
- Documentation updates
- Team training

---

## Risk Mitigation

### Risk 1: Behavioral Differences

**Mitigation:**
- Extensive integration testing
- Side-by-side comparison in staging
- Feature flags for gradual rollout
- Keep Go implementation as fallback

### Risk 2: Performance Regressions

**Mitigation:**
- Comprehensive benchmarking
- Load testing before rollout
- Continuous performance monitoring
- Rollback capability

### Risk 3: Stability Issues

**Mitigation:**
- Long-running soak tests
- Fuzzing for edge cases
- Code review by Rust experts
- Gradual canary deployment

### Risk 4: Team Expertise

**Mitigation:**
- Rust training for team members
- Pair programming during development
- External Rust consultant review
- Comprehensive documentation

### Risk 5: Maintenance Burden

**Mitigation:**
- Well-documented code
- Clear separation of concerns
- Automated testing
- CI/CD for both Go and Rust

---

## Success Metrics

### Performance Metrics

- [ ] IPVS update latency: <1ms (vs 5-10ms in Go+CGo)
- [ ] VRRP timing jitter: <1ms (vs 10-50ms in Go)
- [ ] Healthcheck throughput: >5000 checks/sec (vs 2000 in Go)
- [ ] Memory usage: <500MB for 10K backends (vs 1GB in Go)
- [ ] CPU usage: <20% at typical load (vs 30% in Go)

### Reliability Metrics

- [ ] Zero unplanned failovers due to GC pauses
- [ ] 99.99% healthcheck accuracy
- [ ] <1 minute recovery from component crash
- [ ] Zero memory leaks in 7-day soak test

### Operational Metrics

- [ ] Documentation complete and reviewed
- [ ] Team trained on Rust codebase
- [ ] CI/CD pipeline functional
- [ ] Monitoring and alerting configured
- [ ] Rollback procedure tested

---

## Timeline Summary

| Phase | Component | Duration | Dependencies |
|-------|-----------|----------|--------------|
| Phase 1 | IPVS Bindings | 4-6 weeks | None |
| Phase 2 | HA VRRP | 3-4 weeks | Phase 1 (optional) |
| Phase 3 | Healthcheck Engine | 6-8 weeks | Phase 1 (optional) |
| Integration | gRPC, Testing | 2-3 weeks | All phases |
| Rollout | Staging → Production | 8-12 weeks | Integration complete |

**Total Estimated Time:** 6-9 months

---

## Appendix A: Rust Dependencies

```toml
[workspace]
members = [
    "crates/ipvs",
    "crates/ipvs-ffi",
    "crates/vrrp",
    "crates/healthcheck",
    "crates/common",
]

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
tonic = "0.11"
prost = "0.12"
netlink-packet-core = "0.7"
netlink-packet-generic = "0.3"
netlink-sys = "0.8"
nix = { version = "0.28", features = ["socket", "net"] }
libc = "0.2"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
async-trait = "0.1"
bytes = "1.5"
futures = "0.3"
```

---

## Appendix B: gRPC Protocol Definitions

```protobuf
// proto/ipvs.proto
syntax = "proto3";
package seesaw.ipvs;

service IPVSManager {
    rpc AddService(Service) returns (Response);
    rpc UpdateService(Service) returns (Response);
    rpc DeleteService(ServiceKey) returns (Response);
    rpc GetServices(GetServicesRequest) returns (ServiceList);
    rpc AddDestination(DestinationRequest) returns (Response);
    rpc UpdateDestination(DestinationRequest) returns (Response);
    rpc DeleteDestination(DestinationRequest) returns (Response);
}

message Service {
    string address = 1;
    string protocol = 2;
    uint32 port = 3;
    uint32 fwmark = 4;
    string scheduler = 5;
    uint32 flags = 6;
    uint32 timeout = 7;
}

// ... etc
```

```protobuf
// proto/vrrp.proto
syntax = "proto3";
package seesaw.vrrp;

service VRRPNode {
    rpc GetStatus(StatusRequest) returns (StatusResponse);
    rpc TriggerFailover(FailoverRequest) returns (FailoverResponse);
    rpc UpdateConfig(VRRPConfig) returns (Response);
    rpc StreamStatus(StreamRequest) returns (stream StatusUpdate);
}

message VRRPConfig {
    bool enabled = 1;
    string local_addr = 2;
    string remote_addr = 3;
    uint32 priority = 4;
    uint32 vrid = 5;
    uint32 advert_interval_ms = 6;
}

// ... etc
```

---

## Appendix C: Directory Structure

```
seesaw/
├── rust/
│   ├── Cargo.toml                 # Workspace manifest
│   ├── Cargo.lock
│   ├── crates/
│   │   ├── common/                # Shared utilities
│   │   │   ├── Cargo.toml
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── error.rs
│   │   │       └── logging.rs
│   │   ├── ipvs/                  # IPVS netlink implementation
│   │   │   ├── Cargo.toml
│   │   │   ├── src/
│   │   │   │   ├── lib.rs
│   │   │   │   ├── types.rs
│   │   │   │   ├── netlink.rs
│   │   │   │   └── manager.rs
│   │   │   ├── tests/
│   │   │   │   └── integration.rs
│   │   │   └── benches/
│   │   │       └── ipvs_bench.rs
│   │   ├── ipvs-ffi/              # FFI bindings (if used)
│   │   │   ├── Cargo.toml
│   │   │   ├── include/
│   │   │   │   └── ipvs.h
│   │   │   └── src/
│   │   │       └── lib.rs
│   │   ├── vrrp/                  # VRRP implementation
│   │   │   ├── Cargo.toml
│   │   │   ├── proto/
│   │   │   │   └── vrrp.proto
│   │   │   ├── src/
│   │   │   │   ├── lib.rs
│   │   │   │   ├── packet.rs
│   │   │   │   ├── socket.rs
│   │   │   │   ├── state_machine.rs
│   │   │   │   └── grpc_server.rs
│   │   │   └── tests/
│   │   │       └── integration.rs
│   │   └── healthcheck/           # Healthcheck engine
│   │       ├── Cargo.toml
│   │       ├── proto/
│   │       │   └── healthcheck.proto
│   │       ├── src/
│   │       │   ├── lib.rs
│   │       │   ├── scheduler.rs
│   │       │   ├── aggregator.rs
│   │       │   ├── grpc.rs
│   │       │   └── checkers/
│   │       │       ├── mod.rs
│   │       │       ├── tcp.rs
│   │       │       ├── http.rs
│   │       │       ├── dns.rs
│   │       │       └── ping.rs
│   │       ├── tests/
│   │       │   └── integration.rs
│   │       └── benches/
│   │           └── check_bench.rs
│   ├── bin/                       # Rust binaries
│   │   ├── ipvs-manager.rs
│   │   ├── vrrp-node.rs
│   │   └── healthcheck-engine.rs
│   └── build.rs                   # Build script for protobuf
├── docs/
│   └── RUST-MIGRATION-PLAN.md    # This document
└── ... (existing Go code)
```

---

## Appendix D: References

- **IPVS Documentation:**
  - Linux kernel docs: https://www.kernel.org/doc/html/latest/networking/ipvs-sysctl.html
  - `ip_vs.h` header: https://elixir.bootlin.com/linux/latest/source/include/uapi/linux/ip_vs.h

- **VRRPv3 RFC:**
  - RFC 5798: https://datatracker.ietf.org/doc/html/rfc5798

- **Rust Crates:**
  - Tokio: https://tokio.rs
  - Tonic (gRPC): https://github.com/hyperium/tonic
  - nix: https://github.com/nix-rust/nix
  - netlink-packet: https://github.com/rust-netlink/netlink-packet-route

- **Existing Implementations:**
  - Rust IPVS: https://github.com/kobolog/ipvs-rs (reference)
  - Rust netlink: https://github.com/rust-netlink

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-02-11 | Initial | Created comprehensive migration plan |

---

**Status:** DRAFT - Awaiting Review

**Next Steps:**
1. Review and approve this plan
2. Set up Rust development environment
3. Begin Phase 1: IPVS Bindings
