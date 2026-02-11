//! IPVS data types and structures.

use std::fmt;
use std::net::IpAddr;

/// IPVS version information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IPVSVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl fmt::Display for IPVSVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// IP protocol for IPVS services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    TCP,
    UDP,
    SCTP,
    Other(u8),
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::TCP => write!(f, "TCP"),
            Protocol::UDP => write!(f, "UDP"),
            Protocol::SCTP => write!(f, "SCTP"),
            Protocol::Other(n) => write!(f, "IP({})", n),
        }
    }
}

/// IPVS scheduling algorithm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scheduler {
    RoundRobin,
    WeightedRoundRobin,
    LeastConnection,
    WeightedLeastConnection,
    SourceHashing,
    MaglevHashing,
    Other(String),
}

impl fmt::Display for Scheduler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scheduler::RoundRobin => write!(f, "rr"),
            Scheduler::WeightedRoundRobin => write!(f, "wrr"),
            Scheduler::LeastConnection => write!(f, "lc"),
            Scheduler::WeightedLeastConnection => write!(f, "wlc"),
            Scheduler::SourceHashing => write!(f, "sh"),
            Scheduler::MaglevHashing => write!(f, "mh"),
            Scheduler::Other(s) => write!(f, "{}", s),
        }
    }
}

/// Service flags for IPVS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServiceFlags(pub u32);

impl ServiceFlags {
    pub const PERSISTENT: u32 = 0x1;
    pub const HASHED: u32 = 0x2;
    pub const ONE_PACKET: u32 = 0x4;
    pub const SCHED_SH_FALLBACK: u32 = 0x8;
    pub const SCHED_SH_PORT: u32 = 0x10;
    pub const SCHED_MH_FALLBACK: u32 = 0x8;
    pub const SCHED_MH_PORT: u32 = 0x10;
}

/// Destination flags for IPVS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationFlags {
    /// NAT mode (masquerading)
    Masq,
    /// Local delivery
    Local,
    /// Tunnel mode (IP-in-IP)
    Tunnel,
    /// Route mode (DSR - Direct Server Return)
    Route,
    /// Bypass
    Bypass,
}

/// Statistics for an IPVS service.
#[derive(Debug, Clone, Default)]
pub struct ServiceStats {
    pub connections: u32,
    pub packets_in: u32,
    pub packets_out: u32,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub cps: u32,
    pub pps_in: u32,
    pub pps_out: u32,
    pub bps_in: u32,
    pub bps_out: u32,
}

/// Statistics for an IPVS destination.
#[derive(Debug, Clone, Default)]
pub struct DestinationStats {
    pub connections: u32,
    pub packets_in: u32,
    pub packets_out: u32,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub cps: u32,
    pub pps_in: u32,
    pub pps_out: u32,
    pub bps_in: u32,
    pub bps_out: u32,
    pub active_conns: u32,
    pub inactive_conns: u32,
    pub persist_conns: u32,
}

/// An IPVS service (virtual server).
#[derive(Debug, Clone)]
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

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.fwmark > 0 {
            write!(f, "FWM {} ({})", self.fwmark, self.scheduler)
        } else {
            write!(
                f,
                "{} {}:{} ({})",
                self.protocol, self.address, self.port, self.scheduler
            )
        }
    }
}

/// An IPVS destination (real server).
#[derive(Debug, Clone)]
pub struct Destination {
    pub address: IpAddr,
    pub port: u16,
    pub weight: u32,
    pub flags: DestinationFlags,
    pub lower_threshold: u32,
    pub upper_threshold: u32,
    pub statistics: DestinationStats,
}

impl fmt::Display for Destination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.address, self.port)
    }
}
