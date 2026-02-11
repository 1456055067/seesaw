//! VRRP data types and structures.
//!
//! Based on RFC 5798: Virtual Router Redundancy Protocol (VRRP) Version 3

use std::net::IpAddr;
use std::time::Duration;

/// VRRP protocol version (3 per RFC 5798)
pub const VRRP_VERSION: u8 = 3;

/// VRRP IP protocol number
pub const VRRP_PROTOCOL: u8 = 112;

/// VRRP multicast address for IPv4
pub const VRRP_MULTICAST_ADDR_V4: &str = "224.0.0.18";

/// VRRP multicast address for IPv6
pub const VRRP_MULTICAST_ADDR_V6: &str = "ff02::12";

/// Default advertisement interval (centiseconds)
pub const DEFAULT_ADVERT_INTERVAL: u16 = 100; // 1 second

/// VRRP state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VRRPState {
    /// Initial state - not yet initialized
    Init,
    /// Backup state - monitoring for master failures
    Backup,
    /// Master state - sending advertisements
    Master,
}

impl std::fmt::Display for VRRPState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VRRPState::Init => write!(f, "INIT"),
            VRRPState::Backup => write!(f, "BACKUP"),
            VRRPState::Master => write!(f, "MASTER"),
        }
    }
}

/// VRRP configuration
#[derive(Debug, Clone)]
pub struct VRRPConfig {
    /// Virtual Router ID (1-255)
    pub vrid: u8,

    /// Priority for this router (1-255, 255 = IP address owner)
    pub priority: u8,

    /// Advertisement interval in centiseconds (default 100 = 1 second)
    pub advert_interval: u16,

    /// Network interface name
    pub interface: String,

    /// Virtual IP addresses to manage
    pub virtual_ips: Vec<IpAddr>,

    /// Whether to preempt lower priority masters
    pub preempt: bool,

    /// Accept mode - accept packets destined for virtual IP even if not master
    pub accept_mode: bool,
}

impl Default for VRRPConfig {
    fn default() -> Self {
        Self {
            vrid: 1,
            priority: 100,
            advert_interval: DEFAULT_ADVERT_INTERVAL,
            interface: String::from("eth0"),
            virtual_ips: Vec::new(),
            preempt: true,
            accept_mode: false,
        }
    }
}

impl VRRPConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.vrid == 0 {
            return Err("VRID must be between 1 and 255".to_string());
        }

        if self.priority == 0 {
            return Err("Priority must be between 1 and 255".to_string());
        }

        if self.virtual_ips.is_empty() {
            return Err("At least one virtual IP is required".to_string());
        }

        if self.interface.is_empty() {
            return Err("Interface name is required".to_string());
        }

        Ok(())
    }

    /// Calculate Master_Down_Interval per RFC 5798 Section 6.1
    ///
    /// Master_Down_Interval = (3 * Advertisement_Interval) + Skew_Time
    /// Skew_Time = ((256 - Priority) * Advertisement_Interval) / 256
    pub fn master_down_interval(&self) -> Duration {
        let advert_ms = (self.advert_interval as u64) * 10; // centiseconds to ms
        let skew_ms = ((256 - self.priority as u64) * advert_ms) / 256;
        let master_down_ms = (3 * advert_ms) + skew_ms;

        Duration::from_millis(master_down_ms)
    }

    /// Calculate Advertisement_Interval in milliseconds
    pub fn advert_interval_ms(&self) -> Duration {
        Duration::from_millis((self.advert_interval as u64) * 10)
    }
}

/// VRRP statistics
#[derive(Debug, Clone, Default)]
pub struct VRRPStats {
    /// Number of transitions to Master state
    pub master_transitions: u64,

    /// Number of transitions to Backup state
    pub backup_transitions: u64,

    /// Advertisements sent (as master)
    pub adverts_sent: u64,

    /// Advertisements received (as backup)
    pub adverts_received: u64,

    /// Invalid advertisements received
    pub invalid_adverts: u64,

    /// Priority zero advertisements received
    pub priority_zero_received: u64,

    /// Checksum errors
    pub checksum_errors: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_down_interval() {
        let config = VRRPConfig {
            vrid: 1,
            priority: 100,
            advert_interval: 100, // 1 second
            ..Default::default()
        };

        // Master_Down_Interval = (3 * 1000) + skew
        // skew = ((256 - 100) * 1000) / 256 = 609.375ms
        // total = 3609.375ms
        let interval = config.master_down_interval();
        assert!(interval.as_millis() >= 3600 && interval.as_millis() <= 3610);
    }

    #[test]
    fn test_config_validation() {
        let mut config = VRRPConfig::default();

        // Should fail - no virtual IPs
        assert!(config.validate().is_err());

        // Add virtual IP
        config.virtual_ips.push("192.168.1.1".parse().unwrap());
        assert!(config.validate().is_ok());

        // Invalid VRID
        config.vrid = 0;
        assert!(config.validate().is_err());
    }
}
