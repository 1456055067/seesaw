//! FFI bindings for VRRP implementation.
//!
//! This crate provides a C-compatible FFI interface to the Rust VRRP implementation.

#![allow(unsafe_op_in_unsafe_fn)]

use std::ffi::{c_char, CStr};
use std::net::IpAddr;
use std::ptr;
use std::sync::Arc;
use tokio::runtime::Runtime;
use vrrp::{VRRPConfig, VRRPNode, VRRPState, VRRPStats};

/// Opaque handle to a VRRP node
pub struct VrrpHandle {
    node: Arc<VRRPNode>,
    runtime: Runtime,
}

/// VRRP configuration for C
#[repr(C)]
pub struct CVrrpConfig {
    /// Virtual Router ID (1-255)
    pub vrid: u8,
    /// Priority (1-255)
    pub priority: u8,
    /// Advertisement interval in centiseconds
    pub advert_interval: u16,
    /// Preempt mode
    pub preempt: bool,
    /// Interface name (null-terminated)
    pub _interface: *const c_char,
    /// Primary IP address (null-terminated string)
    pub primary_ip: *const c_char,
    /// Virtual IP addresses (null-terminated strings)
    pub virtual_ips: *const *const c_char,
    /// Number of virtual IPs
    pub virtual_ip_count: usize,
}

/// VRRP state for C
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CVrrpState {
    Init = 0,
    Backup = 1,
    Master = 2,
}

impl From<VRRPState> for CVrrpState {
    fn from(state: VRRPState) -> Self {
        match state {
            VRRPState::Init => CVrrpState::Init,
            VRRPState::Backup => CVrrpState::Backup,
            VRRPState::Master => CVrrpState::Master,
        }
    }
}

/// VRRP statistics for C
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct CVrrpStats {
    pub master_transitions: u64,
    pub backup_transitions: u64,
    pub adverts_sent: u64,
    pub adverts_received: u64,
    pub invalid_adverts: u64,
    pub priority_zero_received: u64,
    pub checksum_errors: u64,
}

impl From<VRRPStats> for CVrrpStats {
    fn from(stats: VRRPStats) -> Self {
        CVrrpStats {
            master_transitions: stats.master_transitions,
            backup_transitions: stats.backup_transitions,
            adverts_sent: stats.adverts_sent,
            adverts_received: stats.adverts_received,
            invalid_adverts: stats.invalid_adverts,
            priority_zero_received: stats.priority_zero_received,
            checksum_errors: stats.checksum_errors,
        }
    }
}

/// Create a new VRRP node
///
/// Returns a handle to the VRRP node, or null on error.
#[unsafe(no_mangle)]
pub extern "C" fn vrrp_new(config: *const CVrrpConfig) -> *mut VrrpHandle {
    if config.is_null() {
        return ptr::null_mut();
    }

    let config = unsafe { &*config };

    // Parse interface name
    let interface = match unsafe { CStr::from_ptr(config._interface) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    // Parse primary IP
    let primary_ip: IpAddr = match unsafe { CStr::from_ptr(config.primary_ip) }.to_str() {
        Ok(s) => match s.parse() {
            Ok(ip) => ip,
            Err(_) => return ptr::null_mut(),
        },
        Err(_) => return ptr::null_mut(),
    };

    // Parse virtual IPs
    let mut virtual_ips = Vec::new();
    for i in 0..config.virtual_ip_count {
        let ip_ptr = unsafe { *config.virtual_ips.add(i) };
        if ip_ptr.is_null() {
            return ptr::null_mut();
        }

        let ip_str = match unsafe { CStr::from_ptr(ip_ptr) }.to_str() {
            Ok(s) => s,
            Err(_) => return ptr::null_mut(),
        };

        let ip: IpAddr = match ip_str.parse() {
            Ok(ip) => ip,
            Err(_) => return ptr::null_mut(),
        };

        virtual_ips.push(ip);
    }

    // Create VRRP config
    let vrrp_config = VRRPConfig {
        vrid: config.vrid,
        priority: config.priority,
        advert_interval: config.advert_interval,
        interface: interface.to_string(),
        virtual_ips,
        preempt: config.preempt,
        accept_mode: false,
    };

    // Validate config
    if vrrp_config.validate().is_err() {
        return ptr::null_mut();
    }

    // Create tokio runtime
    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return ptr::null_mut(),
    };

    // Create VRRP node
    let node = match VRRPNode::new(vrrp_config, interface, primary_ip) {
        Ok(n) => Arc::new(n),
        Err(_) => return ptr::null_mut(),
    };

    Box::into_raw(Box::new(VrrpHandle { node, runtime }))
}

/// Free a VRRP node handle
#[unsafe(no_mangle)]
pub extern "C" fn vrrp_free(handle: *mut VrrpHandle) {
    if !handle.is_null() {
        let handle = unsafe { Box::from_raw(handle) };

        // Shutdown the node
        let _ = handle.runtime.block_on(async {
            handle.node.shutdown().await
        });

        drop(handle);
    }
}

/// Run the VRRP state machine
///
/// This function blocks until the VRRP node terminates or an error occurs.
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn vrrp_run(handle: *mut VrrpHandle) -> i32 {
    if handle.is_null() {
        return -1;
    }

    let handle = unsafe { &*handle };

    match handle.runtime.block_on(async {
        handle.node.run().await
    }) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Run the VRRP state machine in a background thread
///
/// Returns a thread handle that can be used to wait for completion.
/// Returns null on error.
#[unsafe(no_mangle)]
pub extern "C" fn vrrp_run_async(handle: *mut VrrpHandle) -> *mut std::thread::JoinHandle<i32> {
    if handle.is_null() {
        return ptr::null_mut();
    }

    let handle = unsafe { &*handle };
    let node = handle.node.clone();

    // Spawn a new thread with its own runtime
    let thread = std::thread::spawn(move || {
        let runtime = match Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return -1,
        };

        match runtime.block_on(async {
            node.run().await
        }) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    });

    Box::into_raw(Box::new(thread))
}

/// Get the current VRRP state
#[unsafe(no_mangle)]
pub extern "C" fn vrrp_get_state(handle: *const VrrpHandle) -> CVrrpState {
    if handle.is_null() {
        return CVrrpState::Init;
    }

    let handle = unsafe { &*handle };

    let state = handle.runtime.block_on(async {
        handle.node.get_state().await
    });

    state.into()
}

/// Get VRRP statistics
///
/// Returns true if successful, false on error.
#[unsafe(no_mangle)]
pub extern "C" fn vrrp_get_stats(handle: *const VrrpHandle, stats: *mut CVrrpStats) -> bool {
    if handle.is_null() || stats.is_null() {
        return false;
    }

    let handle = unsafe { &*handle };

    let vrrp_stats = handle.runtime.block_on(async {
        handle.node.get_stats().await
    });

    unsafe {
        *stats = vrrp_stats.into();
    }

    true
}

/// Shutdown the VRRP node gracefully
///
/// Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn vrrp_shutdown(handle: *mut VrrpHandle) -> i32 {
    if handle.is_null() {
        return -1;
    }

    let handle = unsafe { &*handle };

    match handle.runtime.block_on(async {
        handle.node.shutdown().await
    }) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Get the last error message
///
/// Returns a null-terminated string describing the last error.
/// The string is valid until the next call to any vrrp_* function.
#[unsafe(no_mangle)]
pub extern "C" fn vrrp_last_error() -> *const c_char {
    // TODO: Implement thread-local error storage
    b"Not implemented\0".as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_conversion() {
        assert_eq!(CVrrpState::from(VRRPState::Init), CVrrpState::Init);
        assert_eq!(CVrrpState::from(VRRPState::Backup), CVrrpState::Backup);
        assert_eq!(CVrrpState::from(VRRPState::Master), CVrrpState::Master);
    }
}
