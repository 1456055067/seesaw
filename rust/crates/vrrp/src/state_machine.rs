//! VRRP state machine implementation.
//!
//! Implements RFC 5798 state transitions:
//! - Init → Backup/Master
//! - Backup → Master (on master_down_timer expiry)
//! - Master → Backup (on higher priority advertisement)

use crate::packet::VRRPPacket;
use crate::socket::VRRPSocket;
use crate::types::{VRRPConfig, VRRPState, VRRPStats};
use std::io;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time::{interval, sleep};
use tracing::{debug, info, warn};

/// VRRP node managing state transitions and advertisements
pub struct VRRPNode {
    config: VRRPConfig,
    state: Arc<RwLock<VRRPState>>,
    stats: Arc<RwLock<VRRPStats>>,
    socket: Arc<VRRPSocket>,
    primary_ip: IpAddr,
}

impl VRRPNode {
    /// Create a new VRRP node
    ///
    /// # Arguments
    /// * `config` - VRRP configuration
    /// * `interface` - Network interface name
    /// * `primary_ip` - Primary IP address of this node
    pub fn new(config: VRRPConfig, interface: &str, primary_ip: IpAddr) -> io::Result<Self> {
        let is_ipv6 = matches!(config.virtual_ips.first(), Some(IpAddr::V6(_)));

        let socket = VRRPSocket::new(interface, is_ipv6)?;
        socket.join_multicast()?;

        Ok(Self {
            config,
            state: Arc::new(RwLock::new(VRRPState::Init)),
            stats: Arc::new(RwLock::new(VRRPStats::default())),
            socket: Arc::new(socket),
            primary_ip,
        })
    }

    /// Get current VRRP state
    pub async fn get_state(&self) -> VRRPState {
        *self.state.read().await
    }

    /// Get statistics
    pub async fn get_stats(&self) -> VRRPStats {
        self.stats.read().await.clone()
    }

    /// Run the VRRP state machine
    pub async fn run(&self) -> io::Result<()> {
        info!(
            vrid = self.config.vrid,
            priority = self.config.priority,
            "Starting VRRP node"
        );

        // Transition from Init to Backup or Master
        self.init_transition().await?;

        // Main state machine loop
        loop {
            match self.get_state().await {
                VRRPState::Init => {
                    // Should not happen after init_transition
                    warn!("Unexpected Init state in main loop");
                    sleep(Duration::from_secs(1)).await;
                }
                VRRPState::Backup => {
                    self.run_backup().await?;
                }
                VRRPState::Master => {
                    self.run_master().await?;
                }
            }
        }
    }

    /// Transition from Init state
    async fn init_transition(&self) -> io::Result<()> {
        if self.config.priority == 255 {
            // Priority 255 = IP address owner, immediately become Master
            info!(vrid = self.config.vrid, "IP owner, transitioning to Master");
            self.transition_to_master().await?;
        } else {
            // Start as Backup and wait for Master_Down_Interval
            info!(vrid = self.config.vrid, "Transitioning to Backup");
            self.transition_to_backup().await?;
        }
        Ok(())
    }

    /// Run Backup state logic
    async fn run_backup(&self) -> io::Result<()> {
        let master_down = self.config.master_down_interval();
        let deadline = Instant::now() + master_down;

        debug!(
            vrid = self.config.vrid,
            master_down_ms = master_down.as_millis(),
            "Backup: waiting for Master_Down_Interval"
        );

        loop {
            // Check for incoming advertisements
            match tokio::time::timeout(Duration::from_millis(10), async { self.socket.try_recv() })
                .await
            {
                Ok(Ok(Some((packet, src_ip)))) => {
                    self.handle_advertisement_backup(&packet, src_ip, &mut deadline.clone())
                        .await?;

                    // Check if we're still in Backup state
                    if self.get_state().await != VRRPState::Backup {
                        return Ok(());
                    }
                }
                Ok(Ok(None)) => {
                    // No packet available, continue
                }
                Ok(Err(e)) => {
                    warn!(error = ?e, "Error receiving packet");
                }
                Err(_) => {
                    // Timeout, check master_down_timer
                }
            }

            // Check if Master_Down_Interval expired
            if Instant::now() >= deadline {
                info!(vrid = self.config.vrid, "Master_Down_Interval expired");
                self.transition_to_master().await?;
                return Ok(());
            }
        }
    }

    /// Handle advertisement received in Backup state
    async fn handle_advertisement_backup(
        &self,
        packet: &VRRPPacket,
        src_ip: IpAddr,
        deadline: &mut Instant,
    ) -> io::Result<()> {
        // Verify VRID matches
        if packet.vrid != self.config.vrid {
            return Ok(());
        }

        // Verify checksum
        let dst_ip = if src_ip.is_ipv6() {
            crate::types::VRRP_MULTICAST_ADDR_V6.parse().unwrap()
        } else {
            crate::types::VRRP_MULTICAST_ADDR_V4.parse().unwrap()
        };

        if !packet.verify_checksum(src_ip, dst_ip) {
            self.stats.write().await.checksum_errors += 1;
            warn!(vrid = self.config.vrid, "Invalid checksum");
            return Ok(());
        }

        self.stats.write().await.adverts_received += 1;

        // Check priority
        if packet.priority == 0 {
            // Master is shutting down
            debug!(vrid = self.config.vrid, "Master shutting down (priority 0)");
            *deadline = Instant::now(); // Trigger immediate Master_Down_Interval expiry
        } else if packet.priority >= self.config.priority || self.config.preempt {
            // Reset Master_Down_Interval
            let master_down = self.config.master_down_interval();
            *deadline = Instant::now() + master_down;
            debug!(
                vrid = self.config.vrid,
                priority = packet.priority,
                "Reset Master_Down_Interval"
            );
        }

        Ok(())
    }

    /// Run Master state logic
    async fn run_master(&self) -> io::Result<()> {
        let advert_interval = Duration::from_millis((self.config.advert_interval as u64) * 10);
        let mut advert_timer = interval(advert_interval);

        info!(
            vrid = self.config.vrid,
            advert_interval_ms = advert_interval.as_millis(),
            "Master: sending advertisements"
        );

        loop {
            tokio::select! {
                _ = advert_timer.tick() => {
                    self.send_advertisement().await?;
                }
                result = async { self.socket.try_recv() } => {
                    if let Ok(Some((packet, src_ip))) = result {
                        self.handle_advertisement_master(&packet, src_ip).await?;

                        // Check if we're still in Master state
                        if self.get_state().await != VRRPState::Master {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    /// Handle advertisement received in Master state
    async fn handle_advertisement_master(
        &self,
        packet: &VRRPPacket,
        src_ip: IpAddr,
    ) -> io::Result<()> {
        // Verify VRID matches
        if packet.vrid != self.config.vrid {
            return Ok(());
        }

        // Verify checksum
        let dst_ip = if src_ip.is_ipv6() {
            crate::types::VRRP_MULTICAST_ADDR_V6.parse().unwrap()
        } else {
            crate::types::VRRP_MULTICAST_ADDR_V4.parse().unwrap()
        };

        if !packet.verify_checksum(src_ip, dst_ip) {
            self.stats.write().await.checksum_errors += 1;
            warn!(vrid = self.config.vrid, "Invalid checksum");
            return Ok(());
        }

        self.stats.write().await.adverts_received += 1;

        // Check if higher priority advertisement received
        if packet.priority > self.config.priority {
            info!(
                vrid = self.config.vrid,
                our_priority = self.config.priority,
                their_priority = packet.priority,
                "Higher priority Master detected, transitioning to Backup"
            );
            self.transition_to_backup().await?;
        } else if packet.priority == self.config.priority {
            // Same priority - check source IP (higher IP wins)
            match (src_ip, self.primary_ip) {
                (IpAddr::V4(their_ip), IpAddr::V4(our_ip)) => {
                    if their_ip > our_ip {
                        info!(
                            vrid = self.config.vrid,
                            "Same priority but higher IP, transitioning to Backup"
                        );
                        self.transition_to_backup().await?;
                    }
                }
                (IpAddr::V6(their_ip), IpAddr::V6(our_ip)) => {
                    if their_ip > our_ip {
                        info!(
                            vrid = self.config.vrid,
                            "Same priority but higher IP, transitioning to Backup"
                        );
                        self.transition_to_backup().await?;
                    }
                }
                _ => {
                    // Mismatched IP versions, ignore
                }
            }
        }

        Ok(())
    }

    /// Send VRRP advertisement
    async fn send_advertisement(&self) -> io::Result<()> {
        let packet = VRRPPacket::new(
            self.config.vrid,
            self.config.priority,
            self.config.advert_interval,
            self.config.virtual_ips.clone(),
        );

        match self.socket.send(&packet, self.primary_ip) {
            Ok(_) => {
                self.stats.write().await.adverts_sent += 1;
                debug!(vrid = self.config.vrid, "Sent advertisement");
                Ok(())
            }
            Err(e) => {
                warn!(vrid = self.config.vrid, error = ?e, "Failed to send advertisement");
                Err(e)
            }
        }
    }

    /// Transition to Master state
    async fn transition_to_master(&self) -> io::Result<()> {
        info!(vrid = self.config.vrid, "Transitioning to Master");
        *self.state.write().await = VRRPState::Master;
        self.stats.write().await.master_transitions += 1;

        // Send gratuitous advertisement immediately
        self.send_advertisement().await?;

        Ok(())
    }

    /// Transition to Backup state
    async fn transition_to_backup(&self) -> io::Result<()> {
        info!(vrid = self.config.vrid, "Transitioning to Backup");
        *self.state.write().await = VRRPState::Backup;
        self.stats.write().await.backup_transitions += 1;
        Ok(())
    }

    /// Graceful shutdown (send priority 0 advertisement)
    pub async fn shutdown(&self) -> io::Result<()> {
        info!(vrid = self.config.vrid, "Shutting down gracefully");

        if self.get_state().await == VRRPState::Master {
            // Send priority 0 advertisement to trigger fast failover
            let packet = VRRPPacket::new(
                self.config.vrid,
                0, // Priority 0 = shutting down
                self.config.advert_interval,
                self.config.virtual_ips.clone(),
            );

            self.socket.send(&packet, self.primary_ip)?;
        }

        *self.state.write().await = VRRPState::Init;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_init_transition_owner() {
        let mut config = VRRPConfig::default();
        config.vrid = 1;
        config.priority = 255; // IP owner
        config.virtual_ips.push("192.168.1.1".parse().unwrap());

        // This will fail without root/CAP_NET_ADMIN, but tests the code path
        let _ = VRRPNode::new(config, "lo", "10.0.0.1".parse().unwrap());
    }

    #[tokio::test]
    async fn test_packet_creation() {
        let ips = vec!["192.168.1.1".parse().unwrap()];
        let packet = VRRPPacket::new(1, 100, 100, ips);

        assert_eq!(packet.vrid, 1);
        assert_eq!(packet.priority, 100);
        assert_eq!(packet.count_ip, 1);
    }
}
