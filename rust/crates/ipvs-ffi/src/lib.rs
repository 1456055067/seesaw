//! FFI (Foreign Function Interface) layer for calling Rust IPVS from Go.
//!
//! This crate provides C-compatible functions that can be called from Go via CGo.
//! All functions use C types and follow C calling conventions.

#![allow(unsafe_op_in_unsafe_fn)]

use ipvs::{
    Destination, DestinationFlags, DestinationStats, IPVSManager, Protocol, Scheduler, Service,
    ServiceFlags, ServiceStats,
};
use std::ffi::CStr;
use std::net::{IpAddr, Ipv4Addr};
use std::os::raw::{c_char, c_int};
use std::ptr;

/// Opaque handle to IPVSManager (passed to Go as a pointer)
pub struct IpvsHandle {
    manager: IPVSManager,
}

/// C-compatible service structure
#[repr(C)]
pub struct CService {
    pub address: u32,      // IPv4 address in network byte order
    pub protocol: u8,      // TCP=6, UDP=17, SCTP=132
    pub port: u16,         // Port in network byte order
    pub fwmark: u32,       // Firewall mark (0 if not used)
    pub scheduler: *const c_char, // Scheduler name (null-terminated)
    pub flags: u32,        // Service flags
    pub timeout: u32,      // Connection timeout
}

/// C-compatible destination structure
#[repr(C)]
pub struct CDestination {
    pub address: u32,          // IPv4 address in network byte order
    pub port: u16,             // Port in network byte order
    pub weight: u32,           // Weight for load balancing
    pub fwd_method: u8,        // Forwarding method: 0=Masq, 1=Local, 2=Tunnel, 3=Route, 4=Bypass
    pub lower_threshold: u32,  // Lower connection threshold
    pub upper_threshold: u32,  // Upper connection threshold
}

/// C-compatible version structure
#[repr(C)]
pub struct CVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

/// Error codes returned to Go
#[repr(C)]
pub enum IpvsError {
    Success = 0,
    NullPointer = -1,
    InvalidUtf8 = -2,
    IpvsError = -3,
    NetlinkError = -4,
    Unknown = -99,
}

/// Create a new IPVS manager instance.
///
/// Returns an opaque handle that must be passed to all other functions.
/// The handle must be freed with ipvs_destroy() when done.
///
/// # Safety
/// This function is safe to call from C/Go.
#[unsafe(no_mangle)]
pub extern "C" fn ipvs_new() -> *mut IpvsHandle {
    match IPVSManager::new() {
        Ok(manager) => {
            let handle = Box::new(IpvsHandle { manager });
            Box::into_raw(handle)
        }
        Err(_) => ptr::null_mut(),
    }
}

/// Destroy an IPVS manager instance and free its resources.
///
/// # Safety
/// The handle must be a valid pointer returned from ipvs_new().
/// After calling this function, the handle is invalid and must not be used.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ipvs_destroy(handle: *mut IpvsHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

/// Get the IPVS kernel version.
///
/// # Safety
/// - handle must be a valid pointer from ipvs_new()
/// - version must be a valid pointer to CVersion
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ipvs_version(
    handle: *mut IpvsHandle,
    version: *mut CVersion,
) -> c_int {
    if handle.is_null() || version.is_null() {
        return IpvsError::NullPointer as c_int;
    }

    let handle = &mut *handle;
    match handle.manager.version() {
        Ok(v) => {
            (*version).major = v.major;
            (*version).minor = v.minor;
            (*version).patch = v.patch;
            IpvsError::Success as c_int
        }
        Err(_) => IpvsError::IpvsError as c_int,
    }
}

/// Flush all IPVS services and destinations.
///
/// # Safety
/// handle must be a valid pointer from ipvs_new()
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ipvs_flush(handle: *mut IpvsHandle) -> c_int {
    if handle.is_null() {
        return IpvsError::NullPointer as c_int;
    }

    let handle = &mut *handle;
    match handle.manager.flush() {
        Ok(_) => IpvsError::Success as c_int,
        Err(_) => IpvsError::IpvsError as c_int,
    }
}

/// Helper to convert CService to Rust Service
unsafe fn c_service_to_rust(c_service: *const CService) -> Result<Service, IpvsError> {
    if c_service.is_null() {
        return Err(IpvsError::NullPointer);
    }

    let c_svc = &*c_service;

    // Parse scheduler name
    let scheduler_cstr = CStr::from_ptr(c_svc.scheduler);
    let scheduler_str = scheduler_cstr
        .to_str()
        .map_err(|_| IpvsError::InvalidUtf8)?;

    let scheduler = match scheduler_str {
        "rr" => Scheduler::RoundRobin,
        "wrr" => Scheduler::WeightedRoundRobin,
        "lc" => Scheduler::LeastConnection,
        "wlc" => Scheduler::WeightedLeastConnection,
        "sh" => Scheduler::SourceHashing,
        "mh" => Scheduler::MaglevHashing,
        other => Scheduler::Other(other.to_string()),
    };

    // Parse protocol
    let protocol = match c_svc.protocol {
        6 => Protocol::TCP,
        17 => Protocol::UDP,
        132 => Protocol::SCTP,
        other => Protocol::Other(other),
    };

    // Convert address from network byte order
    let address = IpAddr::V4(Ipv4Addr::from(u32::from_be(c_svc.address)));

    Ok(Service {
        address,
        protocol,
        port: u16::from_be(c_svc.port),
        fwmark: c_svc.fwmark,
        scheduler,
        flags: ServiceFlags(c_svc.flags),
        timeout: c_svc.timeout,
        persistence_engine: None,
        statistics: ServiceStats::default(),
    })
}

/// Helper to convert CDestination to Rust Destination
unsafe fn c_dest_to_rust(c_dest: *const CDestination) -> Result<Destination, IpvsError> {
    if c_dest.is_null() {
        return Err(IpvsError::NullPointer);
    }

    let c_dst = &*c_dest;

    let flags = match c_dst.fwd_method {
        0 => DestinationFlags::Masq,
        1 => DestinationFlags::Local,
        2 => DestinationFlags::Tunnel,
        3 => DestinationFlags::Route,
        4 => DestinationFlags::Bypass,
        _ => DestinationFlags::Route, // Default
    };

    let address = IpAddr::V4(Ipv4Addr::from(u32::from_be(c_dst.address)));

    Ok(Destination {
        address,
        port: u16::from_be(c_dst.port),
        weight: c_dst.weight,
        flags,
        lower_threshold: c_dst.lower_threshold,
        upper_threshold: c_dst.upper_threshold,
        statistics: DestinationStats::default(),
    })
}

/// Add a new virtual service.
///
/// # Safety
/// - handle must be a valid pointer from ipvs_new()
/// - service must be a valid pointer to CService
/// - service.scheduler must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ipvs_add_service(
    handle: *mut IpvsHandle,
    service: *const CService,
) -> c_int {
    if handle.is_null() {
        return IpvsError::NullPointer as c_int;
    }

    let rust_service = match c_service_to_rust(service) {
        Ok(s) => s,
        Err(e) => return e as c_int,
    };

    let handle = &mut *handle;
    match handle.manager.add_service(&rust_service) {
        Ok(_) => IpvsError::Success as c_int,
        Err(_) => IpvsError::IpvsError as c_int,
    }
}

/// Update an existing virtual service.
///
/// # Safety
/// - handle must be a valid pointer from ipvs_new()
/// - service must be a valid pointer to CService
/// - service.scheduler must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ipvs_update_service(
    handle: *mut IpvsHandle,
    service: *const CService,
) -> c_int {
    if handle.is_null() {
        return IpvsError::NullPointer as c_int;
    }

    let rust_service = match c_service_to_rust(service) {
        Ok(s) => s,
        Err(e) => return e as c_int,
    };

    let handle = &mut *handle;
    match handle.manager.update_service(&rust_service) {
        Ok(_) => IpvsError::Success as c_int,
        Err(_) => IpvsError::IpvsError as c_int,
    }
}

/// Delete a virtual service.
///
/// # Safety
/// - handle must be a valid pointer from ipvs_new()
/// - service must be a valid pointer to CService
/// - service.scheduler must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ipvs_delete_service(
    handle: *mut IpvsHandle,
    service: *const CService,
) -> c_int {
    if handle.is_null() {
        return IpvsError::NullPointer as c_int;
    }

    let rust_service = match c_service_to_rust(service) {
        Ok(s) => s,
        Err(e) => return e as c_int,
    };

    let handle = &mut *handle;
    match handle.manager.delete_service(&rust_service) {
        Ok(_) => IpvsError::Success as c_int,
        Err(_) => IpvsError::IpvsError as c_int,
    }
}

/// Add a destination (backend server) to a service.
///
/// # Safety
/// - handle must be a valid pointer from ipvs_new()
/// - service must be a valid pointer to CService
/// - dest must be a valid pointer to CDestination
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ipvs_add_destination(
    handle: *mut IpvsHandle,
    service: *const CService,
    dest: *const CDestination,
) -> c_int {
    if handle.is_null() {
        return IpvsError::NullPointer as c_int;
    }

    let rust_service = match c_service_to_rust(service) {
        Ok(s) => s,
        Err(e) => return e as c_int,
    };

    let rust_dest = match c_dest_to_rust(dest) {
        Ok(d) => d,
        Err(e) => return e as c_int,
    };

    let handle = &mut *handle;
    match handle.manager.add_destination(&rust_service, &rust_dest) {
        Ok(_) => IpvsError::Success as c_int,
        Err(_) => IpvsError::IpvsError as c_int,
    }
}

/// Update a destination (backend server) in a service.
///
/// # Safety
/// - handle must be a valid pointer from ipvs_new()
/// - service must be a valid pointer to CService
/// - dest must be a valid pointer to CDestination
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ipvs_update_destination(
    handle: *mut IpvsHandle,
    service: *const CService,
    dest: *const CDestination,
) -> c_int {
    if handle.is_null() {
        return IpvsError::NullPointer as c_int;
    }

    let rust_service = match c_service_to_rust(service) {
        Ok(s) => s,
        Err(e) => return e as c_int,
    };

    let rust_dest = match c_dest_to_rust(dest) {
        Ok(d) => d,
        Err(e) => return e as c_int,
    };

    let handle = &mut *handle;
    match handle.manager.update_destination(&rust_service, &rust_dest) {
        Ok(_) => IpvsError::Success as c_int,
        Err(_) => IpvsError::IpvsError as c_int,
    }
}

/// Delete a destination (backend server) from a service.
///
/// # Safety
/// - handle must be a valid pointer from ipvs_new()
/// - service must be a valid pointer to CService
/// - dest must be a valid pointer to CDestination
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ipvs_delete_destination(
    handle: *mut IpvsHandle,
    service: *const CService,
    dest: *const CDestination,
) -> c_int {
    if handle.is_null() {
        return IpvsError::NullPointer as c_int;
    }

    let rust_service = match c_service_to_rust(service) {
        Ok(s) => s,
        Err(e) => return e as c_int,
    };

    let rust_dest = match c_dest_to_rust(dest) {
        Ok(d) => d,
        Err(e) => return e as c_int,
    };

    let handle = &mut *handle;
    match handle.manager.delete_destination(&rust_service, &rust_dest) {
        Ok(_) => IpvsError::Success as c_int,
        Err(_) => IpvsError::IpvsError as c_int,
    }
}

/// Get a human-readable error message for the last error.
///
/// Returns a pointer to a static string. Do not free this pointer.
#[unsafe(no_mangle)]
pub extern "C" fn ipvs_error_string(error_code: c_int) -> *const c_char {
    let msg = match error_code {
        0 => "Success\0",
        -1 => "Null pointer\0",
        -2 => "Invalid UTF-8\0",
        -3 => "IPVS error\0",
        -4 => "Netlink error\0",
        _ => "Unknown error\0",
    };
    msg.as_ptr() as *const c_char
}
