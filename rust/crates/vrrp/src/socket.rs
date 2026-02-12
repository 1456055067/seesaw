//! VRRP socket handling for multicast communication.
//!
//! Implements raw socket creation, multicast group management, and
//! send/receive operations for VRRP advertisements (RFC 5798).

use crate::packet::VRRPPacket;
use crate::types::{VRRP_MULTICAST_ADDR_V4, VRRP_MULTICAST_ADDR_V6, VRRP_PROTOCOL};
use socket2::{Domain, Protocol, Socket, Type};
use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::fd::AsRawFd;

/// VRRP socket for sending and receiving advertisements
pub struct VRRPSocket {
    socket: Socket,
    interface: String,
    is_ipv6: bool,
}

impl VRRPSocket {
    /// Create a new VRRP socket
    ///
    /// # Arguments
    /// * `interface` - Network interface name (e.g., "eth0")
    /// * `is_ipv6` - Whether to use IPv6 (true) or IPv4 (false)
    pub fn new(interface: &str, is_ipv6: bool) -> io::Result<Self> {
        let domain = if is_ipv6 { Domain::IPV6 } else { Domain::IPV4 };

        // Create raw socket for VRRP protocol
        let socket = Socket::new(
            domain,
            Type::RAW,
            Some(Protocol::from(VRRP_PROTOCOL as i32)),
        )?;

        // Set socket options
        socket.set_nonblocking(true)?;

        // Allow multiple VRRP instances to bind to the same address
        socket.set_reuse_address(true)?;

        let fd = socket.as_raw_fd();

        if is_ipv6 {
            // IPv6-specific options
            // Set hop limit to 255 (required by RFC 5798)
            let hop_limit: libc::c_int = 255;
            unsafe {
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IPV6,
                    libc::IPV6_UNICAST_HOPS,
                    &hop_limit as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }

                // Set multicast hop limit
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IPV6,
                    libc::IPV6_MULTICAST_HOPS,
                    &hop_limit as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }

                // Don't loop multicast packets back to sender
                let loop_val: libc::c_int = 0;
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IPV6,
                    libc::IPV6_MULTICAST_LOOP,
                    &loop_val as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
        } else {
            // IPv4-specific options
            // Set TTL to 255 (required by RFC 5798)
            let ttl: libc::c_int = 255;
            unsafe {
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IP,
                    libc::IP_TTL,
                    &ttl as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }

                // Set multicast TTL
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IP,
                    libc::IP_MULTICAST_TTL,
                    &ttl as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }

                // Don't loop multicast packets back to sender
                let loop_val: libc::c_int = 0;
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IP,
                    libc::IP_MULTICAST_LOOP,
                    &loop_val as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
        }

        Ok(Self {
            socket,
            interface: interface.to_string(),
            is_ipv6,
        })
    }

    /// Join the VRRP multicast group
    pub fn join_multicast(&self) -> io::Result<()> {
        let iface_index = get_interface_index(&self.interface)?;

        if self.is_ipv6 {
            let mcast_addr: Ipv6Addr = VRRP_MULTICAST_ADDR_V6.parse().unwrap();
            let fd = self.socket.as_raw_fd();

            // Join IPv6 multicast group
            let mreq = libc::ipv6_mreq {
                ipv6mr_multiaddr: libc::in6_addr {
                    s6_addr: mcast_addr.octets(),
                },
                ipv6mr_interface: iface_index,
            };

            unsafe {
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IPV6,
                    libc::IPV6_ADD_MEMBERSHIP,
                    &mreq as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::ipv6_mreq>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
        } else {
            let mcast_addr: Ipv4Addr = VRRP_MULTICAST_ADDR_V4.parse().unwrap();
            let fd = self.socket.as_raw_fd();

            // Join IPv4 multicast group
            let mreq = libc::ip_mreqn {
                imr_multiaddr: libc::in_addr {
                    s_addr: u32::from_be_bytes(mcast_addr.octets()),
                },
                imr_address: libc::in_addr { s_addr: 0 },
                imr_ifindex: iface_index as i32,
            };

            unsafe {
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IP,
                    libc::IP_ADD_MEMBERSHIP,
                    &mreq as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::ip_mreqn>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }

                // Set multicast interface
                let mcast_if = libc::in_addr {
                    s_addr: u32::from_be_bytes(mcast_addr.octets()),
                };
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IP,
                    libc::IP_MULTICAST_IF,
                    &mcast_if as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::in_addr>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
        }

        Ok(())
    }

    /// Leave the VRRP multicast group
    pub fn leave_multicast(&self) -> io::Result<()> {
        let iface_index = get_interface_index(&self.interface)?;

        if self.is_ipv6 {
            let mcast_addr: Ipv6Addr = VRRP_MULTICAST_ADDR_V6.parse().unwrap();
            let fd = self.socket.as_raw_fd();

            let mreq = libc::ipv6_mreq {
                ipv6mr_multiaddr: libc::in6_addr {
                    s6_addr: mcast_addr.octets(),
                },
                ipv6mr_interface: iface_index,
            };

            unsafe {
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IPV6,
                    libc::IPV6_DROP_MEMBERSHIP,
                    &mreq as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::ipv6_mreq>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
        } else {
            let mcast_addr: Ipv4Addr = VRRP_MULTICAST_ADDR_V4.parse().unwrap();
            let fd = self.socket.as_raw_fd();

            let mreq = libc::ip_mreqn {
                imr_multiaddr: libc::in_addr {
                    s_addr: u32::from_be_bytes(mcast_addr.octets()),
                },
                imr_address: libc::in_addr { s_addr: 0 },
                imr_ifindex: iface_index as i32,
            };

            unsafe {
                if libc::setsockopt(
                    fd,
                    libc::IPPROTO_IP,
                    libc::IP_DROP_MEMBERSHIP,
                    &mreq as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::ip_mreqn>() as libc::socklen_t,
                ) < 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
        }

        Ok(())
    }

    /// Send a VRRP advertisement packet
    pub fn send(&self, packet: &VRRPPacket, src_ip: IpAddr) -> io::Result<usize> {
        let dst_addr = if self.is_ipv6 {
            SocketAddr::new(VRRP_MULTICAST_ADDR_V6.parse().unwrap(), 0)
        } else {
            SocketAddr::new(VRRP_MULTICAST_ADDR_V4.parse().unwrap(), 0)
        };

        // Calculate checksum before sending
        let mut pkt_with_checksum = packet.clone();
        let dst_ip = dst_addr.ip();
        pkt_with_checksum.set_checksum(src_ip, dst_ip);

        let bytes = pkt_with_checksum.to_bytes();
        self.socket.send_to(&bytes, &dst_addr.into())
    }

    /// Receive a VRRP advertisement packet
    ///
    /// Returns the parsed packet and source IP address
    pub fn recv(&self) -> io::Result<(VRRPPacket, IpAddr)> {
        use std::mem::MaybeUninit;

        let mut buf: [MaybeUninit<u8>; 1500] = unsafe { MaybeUninit::uninit().assume_init() };

        let (len, src_addr) = self.socket.recv_from(&mut buf)?;

        // Convert MaybeUninit to initialized data
        let buf: [u8; 1500] = unsafe { std::mem::transmute(buf) };

        if len < 8 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "Packet too short for VRRP",
            ));
        }

        let packet = VRRPPacket::parse(&buf[..len])
            .map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;

        let src_ip = match src_addr.as_socket() {
            Some(addr) => addr.ip(),
            None => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "Invalid source address",
                ));
            }
        };

        Ok((packet, src_ip))
    }

    /// Try to receive a packet without blocking
    ///
    /// Returns None if no packet is available
    pub fn try_recv(&self) -> io::Result<Option<(VRRPPacket, IpAddr)>> {
        match self.recv() {
            Ok(result) => Ok(Some(result)),
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl Drop for VRRPSocket {
    fn drop(&mut self) {
        // Best effort to leave multicast group on cleanup
        let _ = self.leave_multicast();
    }
}

/// Get the interface index for a given interface name
fn get_interface_index(name: &str) -> io::Result<u32> {
    use std::ffi::CString;

    let c_name = CString::new(name).map_err(|e| io::Error::new(ErrorKind::InvalidInput, e))?;

    let index = unsafe { libc::if_nametoindex(c_name.as_ptr()) };

    if index == 0 {
        Err(io::Error::new(
            ErrorKind::NotFound,
            format!("Interface {} not found", name),
        ))
    } else {
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interface_index() {
        // "lo" (loopback) should always exist
        let result = get_interface_index("lo");
        assert!(result.is_ok());
        assert!(result.unwrap() > 0);

        // Non-existent interface
        let result = get_interface_index("nonexistent99");
        assert!(result.is_err());
    }

    #[test]
    fn test_socket_creation() {
        // IPv4 socket
        let result = VRRPSocket::new("lo", false);
        // May fail if not running as root, so just check it doesn't panic
        let _ = result;

        // IPv6 socket
        let result = VRRPSocket::new("lo", true);
        let _ = result;
    }

    #[test]
    fn test_packet_roundtrip() {
        let ips = vec!["192.168.1.1".parse().unwrap()];
        let packet = VRRPPacket::new(1, 100, 100, ips);

        let src_ip: IpAddr = "10.0.0.1".parse().unwrap();
        let dst_ip: IpAddr = VRRP_MULTICAST_ADDR_V4.parse().unwrap();

        let mut pkt_with_checksum = packet.clone();
        pkt_with_checksum.set_checksum(src_ip, dst_ip);

        let bytes = pkt_with_checksum.to_bytes();
        let parsed = VRRPPacket::parse(&bytes).unwrap();

        assert_eq!(parsed.vrid, 1);
        assert_eq!(parsed.priority, 100);
        assert!(parsed.verify_checksum(src_ip, dst_ip));
    }
}
