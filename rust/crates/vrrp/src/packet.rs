//! VRRP packet format and parsing.
//!
//! RFC 5798 Section 5.1 - VRRP Packet Format
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |Version| Type  | Virtual Rtr ID|   Priority    | Count IP Addrs|
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |(rsvd) |     Max Adver Int     |          Checksum             |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                                                               |
//! +                                                               +
//! |                       IP Address(es)                          |
//! +                                                               +
//! |                                                               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use crate::types::VRRP_VERSION;
use bytes::{BufMut, Bytes, BytesMut};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// VRRP packet type (always 1 = ADVERTISEMENT)
const VRRP_TYPE_ADVERTISEMENT: u8 = 1;

/// VRRP packet header (first 8 bytes before IP addresses)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VRRPPacket {
    /// Version (4 bits, always 3) and Type (4 bits, always 1)
    pub version_type: u8,

    /// Virtual Router ID (1-255)
    pub vrid: u8,

    /// Priority (1-255, 0 = master shutting down)
    pub priority: u8,

    /// Count of IP addresses (number of virtual IPs)
    pub count_ip: u8,

    /// Maximum advertisement interval in centiseconds
    pub max_advert_int: u16,

    /// Checksum (covers entire VRRP packet including pseudo-header)
    pub checksum: u16,

    /// Virtual IP addresses
    pub ip_addresses: Vec<IpAddr>,
}

impl VRRPPacket {
    /// Create a new VRRP advertisement packet
    pub fn new(vrid: u8, priority: u8, advert_interval: u16, ips: Vec<IpAddr>) -> Self {
        let version_type = (VRRP_VERSION << 4) | VRRP_TYPE_ADVERTISEMENT;

        Self {
            version_type,
            vrid,
            priority,
            count_ip: ips.len() as u8,
            max_advert_int: advert_interval,
            checksum: 0, // Will be calculated separately
            ip_addresses: ips,
        }
    }

    /// Parse a VRRP packet from raw bytes
    pub fn parse(data: &[u8]) -> Result<Self, &'static str> {
        if data.len() < 8 {
            return Err("Packet too short");
        }

        let version_type = data[0];
        let version = version_type >> 4;
        let pkt_type = version_type & 0x0F;

        if version != VRRP_VERSION {
            return Err("Invalid VRRP version");
        }

        if pkt_type != VRRP_TYPE_ADVERTISEMENT {
            return Err("Invalid packet type");
        }

        let vrid = data[1];
        let priority = data[2];
        let count_ip = data[3];
        let max_advert_int = u16::from_be_bytes([data[4] & 0x0F, data[5]]);
        let checksum = u16::from_be_bytes([data[6], data[7]]);

        // Parse IP addresses
        let mut ip_addresses = Vec::with_capacity(count_ip as usize);
        let mut offset = 8;

        // Determine IP version from packet length
        let expected_len_v4 = 8 + (count_ip as usize * 4);
        let expected_len_v6 = 8 + (count_ip as usize * 16);

        if data.len() == expected_len_v4 {
            // IPv4 addresses
            for _ in 0..count_ip {
                if offset + 4 > data.len() {
                    return Err("Truncated IPv4 address");
                }
                let addr = Ipv4Addr::new(data[offset], data[offset + 1], data[offset + 2], data[offset + 3]);
                ip_addresses.push(IpAddr::V4(addr));
                offset += 4;
            }
        } else if data.len() == expected_len_v6 {
            // IPv6 addresses
            for _ in 0..count_ip {
                if offset + 16 > data.len() {
                    return Err("Truncated IPv6 address");
                }
                let mut octets = [0u8; 16];
                octets.copy_from_slice(&data[offset..offset + 16]);
                let addr = Ipv6Addr::from(octets);
                ip_addresses.push(IpAddr::V6(addr));
                offset += 16;
            }
        } else {
            return Err("Invalid packet length");
        }

        Ok(Self {
            version_type,
            vrid,
            priority,
            count_ip,
            max_advert_int,
            checksum,
            ip_addresses,
        })
    }

    /// Serialize packet to bytes (without checksum)
    pub fn to_bytes(&self) -> Bytes {
        let ip_len = match self.ip_addresses.first() {
            Some(IpAddr::V4(_)) => self.count_ip as usize * 4,
            Some(IpAddr::V6(_)) => self.count_ip as usize * 16,
            None => 0,
        };

        let mut buf = BytesMut::with_capacity(8 + ip_len);

        // Header
        buf.put_u8(self.version_type);
        buf.put_u8(self.vrid);
        buf.put_u8(self.priority);
        buf.put_u8(self.count_ip);

        // Max advert interval (12 bits) + reserved (4 bits)
        buf.put_u16(self.max_advert_int);

        // Checksum
        buf.put_u16(self.checksum);

        // IP addresses
        for ip in &self.ip_addresses {
            match ip {
                IpAddr::V4(addr) => {
                    buf.put_slice(&addr.octets());
                }
                IpAddr::V6(addr) => {
                    buf.put_slice(&addr.octets());
                }
            }
        }

        buf.freeze()
    }

    /// Calculate RFC 1071 checksum over VRRP packet + pseudo-header
    ///
    /// The pseudo-header for IPv4 is:
    /// - Source IP (4 bytes)
    /// - Destination IP (4 bytes)
    /// - Zero (1 byte)
    /// - Protocol (1 byte)
    /// - VRRP Length (2 bytes)
    pub fn calculate_checksum(&self, src_ip: IpAddr, dst_ip: IpAddr) -> u16 {
        let packet_bytes = self.to_bytes();
        let mut sum: u32 = 0;

        // Pseudo-header
        match (src_ip, dst_ip) {
            (IpAddr::V4(src), IpAddr::V4(dst)) => {
                // Source IP
                sum += u16::from_be_bytes([src.octets()[0], src.octets()[1]]) as u32;
                sum += u16::from_be_bytes([src.octets()[2], src.octets()[3]]) as u32;

                // Destination IP
                sum += u16::from_be_bytes([dst.octets()[0], dst.octets()[1]]) as u32;
                sum += u16::from_be_bytes([dst.octets()[2], dst.octets()[3]]) as u32;

                // Zero + Protocol
                sum += crate::types::VRRP_PROTOCOL as u32;

                // VRRP Length
                sum += packet_bytes.len() as u32;
            }
            (IpAddr::V6(src), IpAddr::V6(dst)) => {
                // IPv6 pseudo-header is similar but larger
                let src_octets = src.octets();
                let dst_octets = dst.octets();

                for i in (0..16).step_by(2) {
                    sum += u16::from_be_bytes([src_octets[i], src_octets[i + 1]]) as u32;
                    sum += u16::from_be_bytes([dst_octets[i], dst_octets[i + 1]]) as u32;
                }

                // Length (upper 2 bytes zero)
                sum += packet_bytes.len() as u32;

                // Next header (VRRP protocol)
                sum += crate::types::VRRP_PROTOCOL as u32;
            }
            _ => return 0, // Mismatched IP versions
        }

        // VRRP packet (set checksum field to zero)
        let mut i = 0;
        while i < packet_bytes.len() {
            if i == 6 {
                // Skip checksum field
                i += 2;
                continue;
            }

            let word = if i + 1 < packet_bytes.len() {
                u16::from_be_bytes([packet_bytes[i], packet_bytes[i + 1]])
            } else {
                u16::from_be_bytes([packet_bytes[i], 0])
            };

            sum += word as u32;
            i += 2;
        }

        // Fold 32-bit sum to 16 bits
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }

        !sum as u16
    }

    /// Set the checksum field
    pub fn set_checksum(&mut self, src_ip: IpAddr, dst_ip: IpAddr) {
        self.checksum = self.calculate_checksum(src_ip, dst_ip);
    }

    /// Verify the checksum
    pub fn verify_checksum(&self, src_ip: IpAddr, dst_ip: IpAddr) -> bool {
        let calculated = self.calculate_checksum(src_ip, dst_ip);
        calculated == self.checksum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_serialize_parse() {
        let ips = vec!["192.168.1.1".parse().unwrap(), "192.168.1.2".parse().unwrap()];
        let mut packet = VRRPPacket::new(1, 100, 100, ips.clone());

        let src_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let dst_ip: IpAddr = "224.0.0.18".parse().unwrap();
        packet.set_checksum(src_ip, dst_ip);

        let bytes = packet.to_bytes();
        let parsed = VRRPPacket::parse(&bytes).unwrap();

        assert_eq!(parsed.vrid, 1);
        assert_eq!(parsed.priority, 100);
        assert_eq!(parsed.max_advert_int, 100);
        assert_eq!(parsed.ip_addresses, ips);
        assert!(parsed.verify_checksum(src_ip, dst_ip));
    }
}
