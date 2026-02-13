//! IPVS netlink message serialization and deserialization.
//!
//! This module provides types that implement the traits required for
//! netlink communication with the IPVS kernel module.

use crate::commands::{IPVSCommand, IPVSInfoAttr, IPVSServiceAttr};
use crate::types::{Protocol, Service};
use netlink_packet_core::{DecodeError, ParseableParametrized};
use netlink_packet_generic::{GenlFamily, GenlHeader};
use netlink_packet_utils::{
    Parseable,
    nla::{Nla, NlaBuffer},
    parsers::{parse_u16, parse_u32},
};
use std::convert::TryInto;
use std::net::IpAddr;

// Import Emitable from utils for use in implementations
use netlink_packet_utils::Emitable as UtilsEmitable;

/// IPVS generic netlink message payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IPVSMessage {
    pub cmd: IPVSCommand,
    pub nlas: Vec<IPVSNla>,
}

impl IPVSMessage {
    pub fn new(cmd: IPVSCommand) -> Self {
        Self {
            cmd,
            nlas: Vec::new(),
        }
    }

    pub fn with_nlas(cmd: IPVSCommand, nlas: Vec<IPVSNla>) -> Self {
        Self { cmd, nlas }
    }
}

/// Top-level IPVS netlink attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum IPVSNla {
    /// Service information (nested attributes)
    Service(Vec<ServiceNla>),
    /// Destination information (nested attributes)
    Dest(Vec<DestNla>),
    /// Info attributes (for GET_INFO command)
    Info(Vec<InfoNla>),
    /// Unknown/unsupported attribute
    Other(u16, Vec<u8>),
}

/// Service-specific netlink attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ServiceNla {
    /// Address family (AF_INET = 2, AF_INET6 = 10)
    AddressFamily(u16),
    /// Protocol (TCP = 6, UDP = 17)
    Protocol(u16),
    /// IPv4 address (big-endian u32)
    Address(u32),
    /// Port number (big-endian u16)
    Port(u16),
    /// Firewall mark
    FirewallMark(u32),
    /// Scheduler name
    Scheduler(String),
    /// Flags and mask (packed as two u32 values)
    Flags(u32, u32),
    /// Timeout
    Timeout(u32),
    /// Network mask
    Netmask(u32),
    /// Unknown/unsupported attribute
    Other(u16, Vec<u8>),
}

/// Destination-specific netlink attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DestNla {
    /// IPv4 address (big-endian u32)
    Address(u32),
    /// Port number (big-endian u16)
    Port(u16),
    /// Forwarding method
    ForwardingMethod(u32),
    /// Weight
    Weight(i32),
    /// Upper threshold
    UpperThreshold(u32),
    /// Lower threshold
    LowerThreshold(u32),
    /// Unknown/unsupported attribute
    Other(u16, Vec<u8>),
}

/// Info-specific netlink attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InfoNla {
    /// IPVS version (encoded as u32)
    Version(u32),
    /// Connection table size
    ConnTableSize(u32),
    /// Unknown/unsupported attribute
    Other(u16, Vec<u8>),
}

// Implement Nla trait for top-level IPVS attributes
impl Nla for IPVSNla {
    fn value_len(&self) -> usize {
        match self {
            Self::Service(nlas) => nlas.iter().map(|nla| nla.buffer_len()).sum(),
            Self::Dest(nlas) => nlas.iter().map(|nla| nla.buffer_len()).sum(),
            Self::Info(nlas) => nlas.iter().map(|nla| nla.buffer_len()).sum(),
            Self::Other(_, bytes) => bytes.len(),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Service(_) => 1, // IPVS_ATTR_SERVICE
            Self::Dest(_) => 2,    // IPVS_ATTR_DEST
            Self::Info(_) => 0,    // Special case - info attrs are top-level
            Self::Other(kind, _) => *kind,
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Service(nlas) => {
                let mut offset = 0;
                for nla in nlas {
                    let len = nla.buffer_len();
                    nla.emit(&mut buffer[offset..offset + len]);
                    offset += len;
                }
            }
            Self::Dest(nlas) => {
                let mut offset = 0;
                for nla in nlas {
                    let len = nla.buffer_len();
                    nla.emit(&mut buffer[offset..offset + len]);
                    offset += len;
                }
            }
            Self::Info(nlas) => {
                let mut offset = 0;
                for nla in nlas {
                    let len = nla.buffer_len();
                    nla.emit(&mut buffer[offset..offset + len]);
                    offset += len;
                }
            }
            Self::Other(_, bytes) => buffer.copy_from_slice(bytes),
        }
    }
}

// Implement Nla trait for ServiceNla
impl Nla for ServiceNla {
    fn value_len(&self) -> usize {
        match self {
            Self::AddressFamily(_) => 2,
            Self::Protocol(_) => 2,
            Self::Address(_) => 4,
            Self::Port(_) => 2,
            Self::FirewallMark(_) => 4,
            Self::Scheduler(s) => s.len() + 1, // null-terminated
            Self::Flags(_, _) => 8,            // two u32 values
            Self::Timeout(_) => 4,
            Self::Netmask(_) => 4,
            Self::Other(_, bytes) => bytes.len(),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::AddressFamily(_) => IPVSServiceAttr::AddressFamily as u16,
            Self::Protocol(_) => IPVSServiceAttr::Protocol as u16,
            Self::Address(_) => IPVSServiceAttr::Address as u16,
            Self::Port(_) => IPVSServiceAttr::Port as u16,
            Self::FirewallMark(_) => IPVSServiceAttr::FirewallMark as u16,
            Self::Scheduler(_) => IPVSServiceAttr::Scheduler as u16,
            Self::Flags(_, _) => IPVSServiceAttr::Flags as u16,
            Self::Timeout(_) => IPVSServiceAttr::Timeout as u16,
            Self::Netmask(_) => IPVSServiceAttr::Netmask as u16,
            Self::Other(kind, _) => *kind,
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::AddressFamily(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::Protocol(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::Address(v) => buffer.copy_from_slice(&v.to_be_bytes()),
            Self::Port(v) => buffer.copy_from_slice(&v.to_be_bytes()),
            Self::FirewallMark(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::Scheduler(s) => {
                buffer[..s.len()].copy_from_slice(s.as_bytes());
                buffer[s.len()] = 0; // null terminator
            }
            Self::Flags(flags, mask) => {
                buffer[..4].copy_from_slice(&flags.to_ne_bytes());
                buffer[4..8].copy_from_slice(&mask.to_ne_bytes());
            }
            Self::Timeout(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::Netmask(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::Other(_, bytes) => buffer.copy_from_slice(bytes),
        }
    }
}

// Implement Nla trait for DestNla
impl Nla for DestNla {
    fn value_len(&self) -> usize {
        match self {
            Self::Address(_) => 4,
            Self::Port(_) => 2,
            Self::ForwardingMethod(_) => 4,
            Self::Weight(_) => 4,
            Self::UpperThreshold(_) => 4,
            Self::LowerThreshold(_) => 4,
            Self::Other(_, bytes) => bytes.len(),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Address(_) => 1,          // IPVS_DEST_ATTR_ADDR
            Self::Port(_) => 2,             // IPVS_DEST_ATTR_PORT
            Self::ForwardingMethod(_) => 3, // IPVS_DEST_ATTR_FWD_METHOD
            Self::Weight(_) => 4,           // IPVS_DEST_ATTR_WEIGHT
            Self::UpperThreshold(_) => 5,   // IPVS_DEST_ATTR_U_THRESH
            Self::LowerThreshold(_) => 6,   // IPVS_DEST_ATTR_L_THRESH
            Self::Other(kind, _) => *kind,
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Address(v) => buffer.copy_from_slice(&v.to_be_bytes()),
            Self::Port(v) => buffer.copy_from_slice(&v.to_be_bytes()),
            Self::ForwardingMethod(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::Weight(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::UpperThreshold(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::LowerThreshold(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::Other(_, bytes) => buffer.copy_from_slice(bytes),
        }
    }
}

// Implement Nla trait for InfoNla
impl Nla for InfoNla {
    fn value_len(&self) -> usize {
        match self {
            Self::Version(_) => 4,
            Self::ConnTableSize(_) => 4,
            Self::Other(_, bytes) => bytes.len(),
        }
    }

    fn kind(&self) -> u16 {
        match self {
            Self::Version(_) => IPVSInfoAttr::Version as u16,
            Self::ConnTableSize(_) => IPVSInfoAttr::ConnTableSize as u16,
            Self::Other(kind, _) => *kind,
        }
    }

    fn emit_value(&self, buffer: &mut [u8]) {
        match self {
            Self::Version(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::ConnTableSize(v) => buffer.copy_from_slice(&v.to_ne_bytes()),
            Self::Other(_, bytes) => buffer.copy_from_slice(bytes),
        }
    }
}

// Parsing implementation for InfoNla
impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for InfoNla {
    type Error = DecodeError;

    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, Self::Error> {
        let payload = buf.value();
        Ok(match buf.kind() {
            x if x == IPVSInfoAttr::Version as u16 => {
                Self::Version(parse_u32(payload).map_err(|_| DecodeError::from("Invalid version"))?)
            }
            x if x == IPVSInfoAttr::ConnTableSize as u16 => Self::ConnTableSize(
                parse_u32(payload).map_err(|_| DecodeError::from("Invalid conn table size"))?,
            ),
            kind => Self::Other(kind, payload.to_vec()),
        })
    }
}

// Parsing implementation for ServiceNla
impl<'a, T: AsRef<[u8]> + ?Sized> Parseable<NlaBuffer<&'a T>> for ServiceNla {
    type Error = DecodeError;

    fn parse(buf: &NlaBuffer<&'a T>) -> Result<Self, Self::Error> {
        let payload = buf.value();
        Ok(match buf.kind() {
            x if x == IPVSServiceAttr::AddressFamily as u16 => Self::AddressFamily(
                parse_u16(payload).map_err(|_| DecodeError::from("Invalid address family"))?,
            ),
            x if x == IPVSServiceAttr::Protocol as u16 => Self::Protocol(
                parse_u16(payload).map_err(|_| DecodeError::from("Invalid protocol"))?,
            ),
            x if x == IPVSServiceAttr::Address as u16 => Self::Address(u32::from_be_bytes(
                payload
                    .try_into()
                    .map_err(|_| DecodeError::from("Invalid address"))?,
            )),
            x if x == IPVSServiceAttr::Port as u16 => Self::Port(u16::from_be_bytes(
                payload
                    .try_into()
                    .map_err(|_| DecodeError::from("Invalid port"))?,
            )),
            x if x == IPVSServiceAttr::Scheduler as u16 => {
                let s = std::str::from_utf8(payload)
                    .map_err(|_| DecodeError::from("Invalid scheduler name"))?
                    .trim_end_matches('\0')
                    .to_string();
                Self::Scheduler(s)
            }
            kind => Self::Other(kind, payload.to_vec()),
        })
    }
}

// Implement Emitable from netlink-packet-core for IPVSMessage
impl netlink_packet_core::Emitable for IPVSMessage {
    fn buffer_len(&self) -> usize {
        self.nlas.iter().map(UtilsEmitable::buffer_len).sum()
    }

    fn emit(&self, buffer: &mut [u8]) {
        let mut offset = 0;
        for nla in &self.nlas {
            let len = UtilsEmitable::buffer_len(nla);
            UtilsEmitable::emit(nla, &mut buffer[offset..offset + len]);
            offset += len;
        }
    }
}

// Implement GenlFamily trait for IPVSMessage
impl GenlFamily for IPVSMessage {
    fn family_name() -> &'static str {
        "IPVS"
    }

    fn version(&self) -> u8 {
        1 // IPVS version
    }

    fn command(&self) -> u8 {
        self.cmd as u8
    }
}

// Implement Parseable for IPVSMessage - parse attributes from buffer
impl ParseableParametrized<[u8], GenlHeader> for IPVSMessage {
    fn parse_with_param(buf: &[u8], header: GenlHeader) -> Result<Self, DecodeError> {
        let cmd = match header.cmd {
            1 => IPVSCommand::GetInfo,
            2 => IPVSCommand::NewService,
            3 => IPVSCommand::SetService,
            4 => IPVSCommand::DelService,
            5 => IPVSCommand::GetService,
            10 => IPVSCommand::Flush,
            _ => return Err(DecodeError::from("Unknown IPVS command")),
        };

        // Parse NLAs based on command type
        let nlas = if buf.is_empty() {
            Vec::new()
        } else {
            match cmd {
                IPVSCommand::GetInfo => {
                    // Info response has top-level info attributes
                    let mut info_nlas = Vec::new();
                    let mut offset = 0;

                    while offset < buf.len() {
                        let nla_buf = NlaBuffer::new(&buf[offset..]);
                        info_nlas.push(InfoNla::parse(&nla_buf)?);
                        offset += nla_buf.length() as usize;
                    }

                    vec![IPVSNla::Info(info_nlas)]
                }
                _ => {
                    // Other commands use service/dest attributes
                    Vec::new()
                }
            }
        };

        Ok(Self { cmd, nlas })
    }
}

// Helper functions for converting between high-level types and NLAs

impl Service {
    /// Convert a Service to netlink attributes for creation/update.
    pub(crate) fn to_service_nlas(&self) -> Vec<ServiceNla> {
        let mut nlas = Vec::new();

        // Address family - AF_INET = 2, AF_INET6 = 10
        let (af, addr_bytes) = match self.address {
            IpAddr::V4(ip) => (libc::AF_INET as u16, u32::from(ip).to_be()),
            IpAddr::V6(_) => {
                // For now, only support IPv4
                // TODO: Add IPv6 support later
                (libc::AF_INET as u16, 0)
            }
        };
        nlas.push(ServiceNla::AddressFamily(af));

        // Protocol - TCP = 6, UDP = 17, SCTP = 132
        let proto = match self.protocol {
            Protocol::TCP => libc::IPPROTO_TCP as u16,
            Protocol::UDP => libc::IPPROTO_UDP as u16,
            Protocol::SCTP => 132, // IPPROTO_SCTP
            Protocol::Other(n) => n as u16,
        };
        nlas.push(ServiceNla::Protocol(proto));

        // Address and port (skip if fwmark is set)
        if self.fwmark == 0 {
            nlas.push(ServiceNla::Address(addr_bytes));
            nlas.push(ServiceNla::Port(self.port.to_be()));
        } else {
            nlas.push(ServiceNla::FirewallMark(self.fwmark));
        }

        // Scheduler
        nlas.push(ServiceNla::Scheduler(format!("{}", self.scheduler)));

        // Flags (flags + mask, both set to flags value)
        nlas.push(ServiceNla::Flags(self.flags.0, self.flags.0));

        // Timeout
        if self.timeout > 0 {
            nlas.push(ServiceNla::Timeout(self.timeout));
        }

        nlas
    }
}

impl crate::types::Destination {
    /// Convert a Destination to netlink attributes for creation/update.
    pub(crate) fn to_dest_nlas(&self) -> Vec<DestNla> {
        let mut nlas = Vec::new();

        // Address
        let addr_bytes = match self.address {
            IpAddr::V4(ip) => u32::from(ip).to_be(),
            IpAddr::V6(_) => {
                // For now, only support IPv4
                // TODO: Add IPv6 support later
                0
            }
        };
        nlas.push(DestNla::Address(addr_bytes));

        // Port
        nlas.push(DestNla::Port(self.port.to_be()));

        // Weight
        nlas.push(DestNla::Weight(self.weight as i32));

        // Forwarding method - convert from DestinationFlags
        let fwd_method = match self.flags {
            crate::types::DestinationFlags::Masq => 0, // IP_VS_CONN_F_MASQ
            crate::types::DestinationFlags::Local => 1, // IP_VS_CONN_F_LOCALNODE
            crate::types::DestinationFlags::Tunnel => 2, // IP_VS_CONN_F_TUNNEL
            crate::types::DestinationFlags::Route => 3, // IP_VS_CONN_F_DROUTE
            crate::types::DestinationFlags::Bypass => 4, // IP_VS_CONN_F_BYPASS
        };
        nlas.push(DestNla::ForwardingMethod(fwd_method));

        // Thresholds
        nlas.push(DestNla::UpperThreshold(self.upper_threshold));
        nlas.push(DestNla::LowerThreshold(self.lower_threshold));

        nlas
    }
}
