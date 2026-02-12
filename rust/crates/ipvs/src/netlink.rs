//! Netlink communication layer for IPVS.
//!
//! This module provides low-level netlink socket operations for communicating
//! with the kernel IPVS module via generic netlink.

use bytes::BytesMut;
use common::{Error, Result};
use netlink_packet_core::{
    NLM_F_REQUEST, NetlinkDeserializable, NetlinkMessage, NetlinkPayload, NetlinkSerializable,
};
use netlink_packet_generic::{
    GenlMessage,
    ctrl::{GenlCtrl, GenlCtrlCmd, nlas::GenlCtrlAttrs},
};
use netlink_sys::{Socket, SocketAddr, protocols::NETLINK_GENERIC};
use tracing::{debug, trace};

use crate::messages::IPVSMessage;

/// IPVS generic netlink family name
const IPVS_GENL_NAME: &str = "IPVS";

/// Netlink socket wrapper for IPVS operations.
pub struct NetlinkSocket {
    socket: Socket,
    family_id: u16,
    sequence: u32,
}

impl NetlinkSocket {
    /// Create a new netlink socket and resolve the IPVS family ID.
    pub fn new() -> Result<Self> {
        debug!("Creating netlink socket for IPVS");

        // Create netlink socket
        let mut socket = Socket::new(NETLINK_GENERIC)
            .map_err(|e| Error::netlink(format!("Failed to create netlink socket: {}", e)))?;

        // Bind to an address
        let addr = SocketAddr::new(0, 0);
        socket
            .bind(&addr)
            .map_err(|e| Error::netlink(format!("Failed to bind netlink socket: {}", e)))?;

        // Connect to kernel
        socket
            .connect(&SocketAddr::new(0, 0))
            .map_err(|e| Error::netlink(format!("Failed to connect netlink socket: {}", e)))?;

        let mut nl_socket = Self {
            socket,
            family_id: 0,
            sequence: 0,
        };

        // Resolve IPVS family ID
        nl_socket.family_id = nl_socket.resolve_family_id(IPVS_GENL_NAME)?;
        debug!("IPVS family ID: {}", nl_socket.family_id);

        Ok(nl_socket)
    }

    /// Get the IPVS family ID.
    pub fn family_id(&self) -> u16 {
        self.family_id
    }

    /// Get the next sequence number.
    fn next_sequence(&mut self) -> u32 {
        self.sequence = self.sequence.wrapping_add(1);
        self.sequence
    }

    /// Resolve a generic netlink family name to its ID.
    fn resolve_family_id(&mut self, family_name: &str) -> Result<u16> {
        debug!("Resolving generic netlink family: {}", family_name);

        // Create CTRL_CMD_GETFAMILY message
        let mut genlmsg: GenlMessage<GenlCtrl> = GenlMessage::from_payload(GenlCtrl {
            cmd: GenlCtrlCmd::GetFamily,
            nlas: vec![GenlCtrlAttrs::FamilyName(family_name.to_string())],
        });

        genlmsg.set_resolved_family_id(libc::GENL_ID_CTRL as u16);

        let mut nlmsg = NetlinkMessage::from(genlmsg);
        nlmsg.header.flags = NLM_F_REQUEST;
        nlmsg.header.sequence_number = self.next_sequence();

        // Send request
        self.send_message(&nlmsg)?;

        // Receive response
        let response: NetlinkMessage<GenlMessage<GenlCtrl>> = self.receive_message()?;

        // Parse response
        match response.payload {
            NetlinkPayload::InnerMessage(genlmsg) => {
                for nla in &genlmsg.payload.nlas {
                    if let GenlCtrlAttrs::FamilyId(id) = nla {
                        trace!("Found family ID {} for {}", id, family_name);
                        return Ok(*id);
                    }
                }
                Err(Error::netlink(format!(
                    "Family ID not found in response for {}",
                    family_name
                )))
            }
            NetlinkPayload::Error(err) => Err(Error::netlink(format!(
                "Netlink error while resolving family: {:?}",
                err.code
            ))),
            _ => Err(Error::netlink("Unexpected netlink response type")),
        }
    }

    /// Send a netlink message.
    fn send_message<T>(&mut self, message: &NetlinkMessage<T>) -> Result<()>
    where
        T: NetlinkSerializable + std::fmt::Debug,
    {
        let mut buf = BytesMut::with_capacity(message.buffer_len());
        message.serialize(&mut buf);

        trace!("Sending netlink message: {:?}", message);

        self.socket
            .send(&buf[..], 0)
            .map_err(|e| Error::netlink(format!("Failed to send netlink message: {}", e)))?;

        Ok(())
    }

    /// Receive a netlink message.
    fn receive_message<T>(&mut self) -> Result<NetlinkMessage<T>>
    where
        T: NetlinkDeserializable + std::fmt::Debug,
    {
        let mut buf = vec![0u8; 8192];

        let len = self
            .socket
            .recv(&mut buf, 0)
            .map_err(|e| Error::netlink(format!("Failed to receive netlink message: {}", e)))?;

        let bytes = &buf[..len];
        let message = NetlinkMessage::<T>::deserialize(bytes)
            .map_err(|e| Error::netlink(format!("Failed to parse netlink message: {}", e)))?;

        trace!("Received netlink message: {:?}", message);

        Ok(message)
    }

    /// Send an IPVS command and receive a response.
    pub fn send_ipvs_command(&mut self, message: IPVSMessage) -> Result<IPVSMessage> {
        let mut genlmsg: GenlMessage<IPVSMessage> = GenlMessage::from_payload(message);
        genlmsg.set_resolved_family_id(self.family_id);

        let mut nlmsg = NetlinkMessage::from(genlmsg);
        nlmsg.header.flags = NLM_F_REQUEST;
        nlmsg.header.sequence_number = self.next_sequence();

        // Send request
        self.send_message(&nlmsg)?;

        // Receive response
        let response: NetlinkMessage<GenlMessage<IPVSMessage>> = self.receive_message()?;

        // Parse response
        match response.payload {
            NetlinkPayload::InnerMessage(genlmsg) => Ok(genlmsg.payload),
            NetlinkPayload::Error(err) => Err(Error::netlink(format!(
                "IPVS command failed: error code {:?}",
                err.code
            ))),
            _ => Err(Error::netlink("Unexpected netlink response type")),
        }
    }
}

impl Drop for NetlinkSocket {
    fn drop(&mut self) {
        // Socket will be closed automatically
        trace!("Closing netlink socket");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_netlink_socket_creation() {
        // This test requires root privileges and IPVS kernel module
        // Skip in CI unless explicitly enabled
        if std::env::var("IPVS_TEST_ENABLED").is_err() {
            eprintln!("Skipping test_netlink_socket_creation (requires IPVS_TEST_ENABLED=1)");
            return;
        }

        let result = NetlinkSocket::new();
        match result {
            Ok(socket) => {
                assert!(socket.family_id() > 0);
                println!("IPVS family ID: {}", socket.family_id());
            }
            Err(e) => {
                panic!("Failed to create netlink socket: {}", e);
            }
        }
    }
}
