# Phase 4: Full Rust Healthcheck Server Rewrite

## Executive Summary

Rewrite the Seesaw healthcheck server entirely in Rust to eliminate FFI overhead and achieve the full **2.1x performance benefit** measured in benchmarks.

**Current state**: Rust checkers accessed via FFI incur ~237µs overhead per check
**Target state**: Native Rust server with direct access to checkers (42µs per check)
**Expected benefit**: **5.4x faster** than current FFI approach, **2.1x faster** than Go

## Architecture Overview

### Current Go Architecture

```
┌─────────────────────────────────────────┐
│ Healthcheck Server (Go)                 │
├─────────────────────────────────────────┤
│ ┌──────────┐  ┌──────────┐  ┌─────────┐│
│ │ Updater  │  │ Manager  │  │Notifier ││
│ │(goroutine│  │(goroutine│  │(goroutn)││
│ └──────────┘  └──────────┘  └─────────┘│
│       │            │              │     │
│       │      ┌─────▼────┐         │     │
│       │      │Healthcheck│         │     │
│       │      │  Pool     │         │     │
│       │      └──────────┘         │     │
└───────┼────────────────────────────┼─────┘
        │                            │
        ▼                            ▼
  ┌──────────┐              ┌──────────────┐
  │ Engine   │              │   Engine     │
  │(RPC Call)│              │(RPC Notify)  │
  └──────────┘              └──────────────┘
```

### Proposed Rust Architecture

```
┌─────────────────────────────────────────┐
│ Healthcheck Server (Rust)               │
├─────────────────────────────────────────┤
│ ┌──────────┐  ┌──────────┐  ┌─────────┐│
│ │ Updater  │  │ Manager  │  │Notifier ││
│ │(tokio    │  │(tokio    │  │(tokio   ││
│ │ task)    │  │ task)    │  │ task)   ││
│ └──────────┘  └──────────┘  └─────────┘│
│       │            │              │     │
│       │      ┌─────▼────┐         │     │
│       │      │ Monitor  │         │     │
│       │      │  Pool    │         │     │
│       │      │(HashMap) │         │     │
│       │      └──────────┘         │     │
└───────┼────────────────────────────┼─────┘
        │                            │
        ▼                            ▼
  ┌──────────┐              ┌──────────────┐
  │ Engine   │              │   Engine     │
  │(Gob/RPC) │              │(Gob/RPC)     │
  └──────────┘              └──────────────┘
```

## Components to Implement

### 1. Server Core (`rust/crates/healthcheck-server/src/server.rs`)

```rust
pub struct HealthcheckServer {
    config: ServerConfig,
    monitors: Arc<RwLock<HashMap<u64, HealthCheckMonitor>>>,
    notify_tx: mpsc::Sender<Notification>,
    engine_socket: String,
}

impl HealthcheckServer {
    pub async fn run(&self) -> Result<()> {
        tokio::select! {
            _ = self.updater() => {},
            _ = self.manager() => {},
            _ = self.notifier() => {},
        }
        Ok(())
    }
}
```

### 2. Engine RPC Client (`rust/crates/healthcheck-server/src/rpc.rs`)

```rust
pub struct EngineClient {
    socket_path: String,
}

impl EngineClient {
    /// Fetch healthcheck configurations from engine
    pub async fn get_healthchecks(&self) -> Result<Vec<HealthCheckConfig>> {
        // Connect to Unix socket
        // Send gob-encoded RPC request
        // Decode gob response
    }

    /// Send health state notifications to engine
    pub async fn send_health_state(&self, notifications: Vec<Notification>) -> Result<()> {
        // Batch notifications
        // Send via RPC
    }
}
```

### 3. Gob Protocol Support

**Challenge**: Go's `encoding/gob` is not directly compatible with Rust

**Options**:
1. **Use JSON instead** - Modify engine to accept JSON (requires Go changes)
2. **Implement gob decoder** - Complex but maintains compatibility
3. **Use gRPC** - Replace RPC with tonic/gRPC (major refactor)
4. **Hybrid approach** - Keep Go RPC proxy, Rust does health checking only

**Recommendation**: Start with **Option 4** (hybrid), migrate to Option 3 (gRPC) long-term

### 4. Hybrid Architecture (Phase 4.1)

```
┌──────────────────┐
│ Thin Go Proxy    │  ← Handles RPC/Gob with Engine
│ (100 LOC)        │
└────────┬─────────┘
         │ (Simple channel interface)
         ▼
┌──────────────────┐
│ Rust Server      │  ← All health checking logic
│ (Main Logic)     │
└──────────────────┘
```

**Benefits**:
- Minimal Go code (just RPC translation layer)
- No gob implementation needed
- Can incrementally migrate
- Get 99% of performance benefit

## Implementation Phases

### Phase 4.1: Hybrid Server (Recommended Start)

**Goal**: Rust server with thin Go RPC proxy

**Tasks**:
1. ✅ Create `healthcheck-server` crate
2. ✅ Implement server core with tokio
3. ✅ Manager task (monitor lifecycle)
4. ✅ Notifier task (batching)
5. ✅ Thin Go proxy for RPC communication
6. ✅ Integration tests
7. ✅ Performance validation (expect 5x improvement)

**Deliverable**: Drop-in replacement for current server with 5x performance

### Phase 4.2: Gob Protocol Support (Optional)

**Goal**: Remove Go dependency entirely

**Tasks**:
1. Implement gob decoder in Rust
2. Direct Unix socket RPC
3. Remove Go proxy
4. Full Rust binary

**Deliverable**: Pure Rust binary, no Go code

### Phase 4.3: gRPC Migration (Future)

**Goal**: Modern RPC protocol

**Tasks**:
1. Define .proto for healthcheck protocol
2. Implement tonic server
3. Modify engine to use gRPC client
4. Remove gob entirely

**Deliverable**: Modern, type-safe RPC

## Expected Performance Improvements

### Current Performance

| Component | Time | Notes |
|-----------|------|-------|
| Check (Go native) | 88 µs | Baseline |
| Check (Rust via FFI) | 325 µs | 3.7x slower (FFI overhead) |

### After Phase 4.1 (Hybrid)

| Component | Time | Improvement |
|-----------|------|-------------|
| Check (Rust native) | **42 µs** | **7.7x faster than FFI** |
|  | | **2.1x faster than Go** |
| RPC overhead | ~10 µs | Go proxy translation |
| Total | **~52 µs** | **6.3x faster than FFI** |

### After Phase 4.2 (Pure Rust)

| Component | Time | Improvement |
|-----------|------|-------------|
| Check (Rust native) | **42 µs** | **7.7x faster than FFI** |
| RPC overhead | 0 | No proxy |
| Total | **42 µs** | **7.7x faster than FFI** |

## Risk Assessment

### High Risk
- **Gob compatibility**: Complex format, no Rust library
  - *Mitigation*: Use hybrid approach (4.1)

### Medium Risk
- **RPC protocol changes**: Breaking compatibility with engine
  - *Mitigation*: Maintain wire compatibility, extensive testing

### Low Risk
- **Tokio runtime**: Well-tested, production-ready
- **Monitor logic**: Already implemented and tested

## Success Criteria

### Phase 4.1 Success Metrics

- [ ] All existing tests pass
- [ ] No functionality regression
- [ ] **5-6x faster** than current FFI approach
- [ ] **2x faster** than Go implementation
- [ ] Memory usage ≤ Go version
- [ ] CPU usage ≤ Go version
- [ ] Can handle 10,000+ concurrent healthchecks
- [ ] Notification batching works correctly
- [ ] Engine communication is reliable

## Migration Path

### Step 1: Development (2-3 days)
1. Implement hybrid server architecture
2. Create thin Go proxy
3. Unit tests for Rust components

### Step 2: Integration (1 day)
1. Integration tests with mock engine
2. Load testing
3. Performance validation

### Step 3: Deployment (Gradual)
1. Deploy to test environment
2. Run alongside Go server (comparison)
3. Gradual rollout to production
4. Monitor metrics

### Step 4: Cleanup
1. Remove old Go server code
2. Update documentation
3. Performance report

## Code Structure

```
rust/crates/healthcheck-server/
├── src/
│   ├── lib.rs              # Public API
│   ├── server.rs           # Main server logic
│   ├── manager.rs          # Monitor lifecycle management
│   ├── notifier.rs         # Notification batching
│   ├── updater.rs          # Config fetching
│   └── proxy.rs            # Go<->Rust bridge types
├── Cargo.toml
└── benches/                # Performance benchmarks

healthcheck/server/
├── main.go                 # Go proxy (Phase 4.1)
└── rust_server.go          # Rust server wrapper

# OR (Phase 4.2)
rust/crates/healthcheck-server/
├── src/
│   ├── main.rs             # Pure Rust binary
│   └── rpc/
│       ├── gob.rs          # Gob decoder
│       └── client.rs       # Engine RPC client
```

## Decision: Proceed with Phase 4.1?

**Pros**:
- **5-6x performance improvement** over current FFI approach
- **2x improvement** over Go
- Maintains compatibility
- Low risk (hybrid approach)
- Incremental migration path
- Keeps existing RPC working

**Cons**:
- Still requires small amount of Go code
- Not "pure" Rust solution
- Adds complexity (two languages)

**Recommendation**: **YES** - Proceed with Phase 4.1

The performance benefit (5-6x) is substantial and the hybrid approach minimizes risk while maintaining compatibility. The thin Go proxy is minimal (~100 LOC) and can be eliminated later in Phase 4.2 if desired.

## Next Steps

1. Review and approve this plan
2. Create `healthcheck-server` crate
3. Implement server core
4. Implement thin Go proxy
5. Integration tests
6. Performance validation
7. Production deployment

---

**Estimated Effort**: 3-4 days for Phase 4.1
**Expected Benefit**: 5-6x faster healthchecking
**Risk Level**: Low (hybrid approach with fallback)
