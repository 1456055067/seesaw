//! IPVS netlink commands and attribute definitions.
//!
//! Based on Linux kernel's include/uapi/linux/ip_vs.h

/// IPVS generic netlink commands
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IPVSCommand {
    /// Get IPVS version and connection table size
    GetInfo = 1,
    /// Add a new virtual service
    NewService = 2,
    /// Modify an existing virtual service
    SetService = 3,
    /// Delete a virtual service
    DelService = 4,
    /// Get virtual service information
    GetService = 5,
    /// Add a new destination to a service
    NewDest = 6,
    /// Modify an existing destination
    SetDest = 7,
    /// Delete a destination from a service
    DelDest = 8,
    /// Get destination information
    GetDest = 9,
    /// Flush all virtual services
    Flush = 10,
    /// Add a new daemon
    NewDaemon = 11,
    /// Delete a daemon
    DelDaemon = 12,
    /// Get daemon information
    GetDaemon = 13,
    /// Set timeout values
    SetTimeout = 14,
    /// Get timeout values
    GetTimeout = 15,
    /// Set daemon configuration
    SetConfig = 16,
    /// Get daemon configuration
    GetConfig = 17,
}

impl From<IPVSCommand> for u8 {
    fn from(cmd: IPVSCommand) -> u8 {
        cmd as u8
    }
}

/// IPVS netlink attributes
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum IPVSAttr {
    Unspec = 0,
    /// Service information (nested)
    Service = 1,
    /// Destination information (nested)
    Dest = 2,
    /// Daemon information (nested)
    Daemon = 3,
    /// Timeout configuration (nested)
    TimeoutTCP = 4,
    TimeoutTCPFin = 5,
    TimeoutUDP = 6,
    /// Daemon sync configuration (nested)
    DaemonSyncConnections = 7,
}

/// Service-specific attributes (nested under IPVS_ATTR_SERVICE)
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum IPVSServiceAttr {
    Unspec = 0,
    /// Address family (AF_INET or AF_INET6)
    AddressFamily = 1,
    /// IP protocol (IPPROTO_TCP, IPPROTO_UDP, etc.)
    Protocol = 2,
    /// Virtual IP address
    Address = 3,
    /// Virtual port
    Port = 4,
    /// Firewall mark
    FirewallMark = 5,
    /// Scheduler name (string)
    Scheduler = 6,
    /// Service flags (u32)
    Flags = 7,
    /// Connection timeout
    Timeout = 8,
    /// Network mask (for persistent connections)
    Netmask = 9,
    /// Service statistics (nested)
    Stats = 10,
    /// Persistence engine name
    PersistenceEngine = 11,
    /// IPv6 address
    AddressV6 = 12,
}

/// Destination-specific attributes (nested under IPVS_ATTR_DEST)
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum IPVSDestAttr {
    Unspec = 0,
    /// Destination IP address
    Address = 1,
    /// Destination port
    Port = 2,
    /// Forwarding method flags
    ForwardingMethod = 3,
    /// Weight
    Weight = 4,
    /// Upper threshold
    UpperThreshold = 5,
    /// Lower threshold
    LowerThreshold = 6,
    /// Active connections count
    ActiveConns = 7,
    /// Inactive connections count
    InactiveConns = 8,
    /// Persistent connections count
    PersistConns = 9,
    /// Destination statistics (nested)
    Stats = 10,
    /// IPv6 address
    AddressV6 = 11,
}

/// Statistics attributes
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
#[allow(dead_code)]
pub enum IPVSStatsAttr {
    Unspec = 0,
    /// Total connections
    Connections = 1,
    /// Packets received
    PacketsIn = 2,
    /// Packets sent
    PacketsOut = 3,
    /// Bytes received
    BytesIn = 4,
    /// Bytes sent
    BytesOut = 5,
    /// Connections per second
    CPS = 6,
    /// Packets per second (in)
    PPSIn = 7,
    /// Packets per second (out)
    PPSOut = 8,
    /// Bytes per second (in)
    BPSIn = 9,
    /// Bytes per second (out)
    BPSOut = 10,
}

/// Info attributes (for IPVS_CMD_GET_INFO)
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum IPVSInfoAttr {
    Unspec = 0,
    /// IPVS version
    Version = 1,
    /// Connection table size
    ConnTableSize = 2,
}
