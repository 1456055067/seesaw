//! Virtual Router Redundancy Protocol (VRRP) Version 3
//!
//! Pure Rust implementation of RFC 5798 for high-availability load balancing.
//!
//! # Features
//!
//! - VRRPv3 protocol implementation (IPv4 and IPv6)
//! - Sub-millisecond failover detection
//! - Priority-based master election
//! - Preemption support
//! - Graceful shutdown (priority 0 advertisements)
//!
//! # Example
//!
//! ```no_run
//! use vrrp::{VRRPConfig, VRRPNode};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut config = VRRPConfig::default();
//! config.vrid = 1;
//! config.priority = 100;
//! config.virtual_ips.push("192.168.1.1".parse()?);
//!
//! // Create VRRP node
//! let node = VRRPNode::new(config, "eth0", "10.0.0.1".parse()?)?;
//!
//! // Run state machine (requires CAP_NET_ADMIN)
//! node.run().await?;
//! # Ok(())
//! # }
//! ```

mod packet;
mod socket;
mod state_machine;
mod types;

pub use packet::VRRPPacket;
pub use socket::VRRPSocket;
pub use state_machine::VRRPNode;
pub use types::{VRRPConfig, VRRPState, VRRPStats, VRRP_VERSION};

// Phase 2.1: Protocol implementation (DONE - types, packet format, checksum)
// Phase 2.2: Socket and multicast (DONE - raw sockets, multicast join/leave)
// Phase 2.3: State machine (DONE - Init/Backup/Master transitions)
// Phase 2.4: Integration (TODO - FFI bridge, Go wrapper)
// Phase 2.5: Testing (TODO - integration tests)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_types() {
        let config = VRRPConfig::default();
        assert_eq!(config.vrid, 1);

        let state = VRRPState::Init;
        assert_eq!(state.to_string(), "INIT");
    }
}
