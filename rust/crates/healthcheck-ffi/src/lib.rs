//! FFI bridge for health checking functionality.
//!
//! This crate provides C-compatible FFI bindings for the healthcheck crate.

use healthcheck::{
    checkers::{DnsChecker, HealthChecker, HttpChecker, TcpChecker},
    monitor::HealthCheckMonitor,
    types::{CheckType, HealthCheckConfig, HealthCheckStats, HealthStatus},
};
use std::ffi::CStr;
use std::net::{IpAddr, SocketAddr};
use std::os::raw::c_char;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

/// Opaque handle to a health check monitor
pub struct HealthCheckHandle {
    monitor: HealthCheckMonitor,
    runtime: Arc<Runtime>,
}

/// C-compatible health check configuration
#[repr(C)]
pub struct CHealthCheckConfig {
    /// Target address (IP:port or hostname:port)
    pub target: *const c_char,

    /// Timeout in milliseconds
    pub timeout_ms: u64,

    /// Interval between checks in milliseconds
    pub interval_ms: u64,

    /// Number of consecutive successes required
    pub rise: u32,

    /// Number of consecutive failures required
    pub fall: u32,

    /// Check type (see CHealthCheckType)
    pub check_type: u8,

    /// HTTP method (for HTTP checks, e.g., "GET", "POST")
    pub http_method: *const c_char,

    /// HTTP path (for HTTP checks, e.g., "/health")
    pub http_path: *const c_char,

    /// Expected HTTP status codes (for HTTP checks)
    pub http_expected_codes: *const u16,

    /// Number of expected HTTP status codes
    pub http_expected_codes_count: usize,

    /// Use HTTPS (for HTTP checks)
    pub http_use_https: bool,

    /// DNS query name (for DNS checks)
    pub dns_query: *const c_char,

    /// Expected IP addresses (for DNS checks)
    pub dns_expected_ips: *const *const c_char,

    /// Number of expected IPs
    pub dns_expected_ips_count: usize,
}

/// C-compatible health check type
#[repr(C)]
pub enum CHealthCheckType {
    Tcp = 0,
    Http = 1,
    Ping = 2,
    Dns = 3,
}

/// C-compatible health status
#[repr(C)]
pub enum CHealthStatus {
    Healthy = 0,
    Unhealthy = 1,
    Timeout = 2,
    Error = 3,
}

impl From<HealthStatus> for CHealthStatus {
    fn from(status: HealthStatus) -> Self {
        match status {
            HealthStatus::Healthy => CHealthStatus::Healthy,
            HealthStatus::Unhealthy => CHealthStatus::Unhealthy,
            HealthStatus::Timeout => CHealthStatus::Timeout,
            HealthStatus::Error => CHealthStatus::Error,
        }
    }
}

/// C-compatible health check statistics
#[repr(C)]
pub struct CHealthCheckStats {
    pub total_checks: u64,
    pub successful_checks: u64,
    pub failed_checks: u64,
    pub timeouts: u64,
    pub avg_response_time_ms: f64,
    pub consecutive_successes: u32,
    pub consecutive_failures: u32,
}

impl From<HealthCheckStats> for CHealthCheckStats {
    fn from(stats: HealthCheckStats) -> Self {
        CHealthCheckStats {
            total_checks: stats.total_checks,
            successful_checks: stats.successful_checks,
            failed_checks: stats.failed_checks,
            timeouts: stats.timeouts,
            avg_response_time_ms: stats.avg_response_time_ms,
            consecutive_successes: stats.consecutive_successes,
            consecutive_failures: stats.consecutive_failures,
        }
    }
}

/// Parse C string to Rust String
unsafe fn parse_c_string(ptr: *const c_char) -> Result<String, Box<dyn std::error::Error>> {
    if ptr.is_null() {
        return Err("null pointer".into());
    }
    unsafe { Ok(CStr::from_ptr(ptr).to_str()?.to_string()) }
}

/// Create a health checker from a C config.
///
/// # Safety
///
/// `c_config` must point to a valid `CHealthCheckConfig`. All pointer fields
/// within (target, http_method, http_path, etc.) must be valid for the
/// duration of this call.
unsafe fn create_checker(
    c_config: &CHealthCheckConfig,
) -> Result<(Arc<dyn HealthChecker>, String), i32> {
    let target = unsafe { parse_c_string(c_config.target).map_err(|_| -1)? };

    let checker: Arc<dyn HealthChecker> = match c_config.check_type {
        0 => {
            // TCP
            let addr = target.parse::<SocketAddr>().map_err(|_| -1)?;
            Arc::new(TcpChecker::new(
                addr,
                Duration::from_millis(c_config.timeout_ms),
            ))
        }
        1 => {
            // HTTP
            let method = unsafe { parse_c_string(c_config.http_method).unwrap_or("GET".into()) };
            let path = unsafe { parse_c_string(c_config.http_path).unwrap_or("/".into()) };

            let expected_codes = if !c_config.http_expected_codes.is_null()
                && c_config.http_expected_codes_count > 0
            {
                unsafe {
                    std::slice::from_raw_parts(
                        c_config.http_expected_codes,
                        c_config.http_expected_codes_count,
                    )
                    .to_vec()
                }
            } else {
                vec![]
            };

            let protocol = if c_config.http_use_https {
                "https"
            } else {
                "http"
            };
            let url = format!("{}://{}{}", protocol, target, path);

            let req_method = match method.to_uppercase().as_str() {
                "GET" => reqwest::Method::GET,
                "POST" => reqwest::Method::POST,
                "HEAD" => reqwest::Method::HEAD,
                "PUT" => reqwest::Method::PUT,
                "DELETE" => reqwest::Method::DELETE,
                _ => reqwest::Method::GET,
            };

            HttpChecker::new(url, req_method, expected_codes, Duration::from_millis(c_config.timeout_ms))
                .map(|c| Arc::new(c) as Arc<dyn HealthChecker>)
                .map_err(|_| -1)?
        }
        3 => {
            // DNS
            let query = unsafe { parse_c_string(c_config.dns_query).map_err(|_| -1)? };

            let expected_ips = if !c_config.dns_expected_ips.is_null()
                && c_config.dns_expected_ips_count > 0
            {
                unsafe {
                    let ips_slice = std::slice::from_raw_parts(
                        c_config.dns_expected_ips,
                        c_config.dns_expected_ips_count,
                    );

                    let mut ips = Vec::new();
                    for ip_ptr in ips_slice {
                        if let Ok(ip_str) = parse_c_string(*ip_ptr)
                            && let Ok(ip) = ip_str.parse::<IpAddr>()
                        {
                            ips.push(ip);
                        }
                    }
                    ips
                }
            } else {
                vec![]
            };

            Arc::new(DnsChecker::new(
                query,
                expected_ips,
                Duration::from_millis(c_config.timeout_ms),
            ))
        }
        _ => return Err(-1),
    };

    Ok((checker, target))
}

/// Create a new health check monitor
///
/// Returns NULL on error. Use healthcheck_free to clean up.
///
/// # Safety
///
/// `config` must be a valid pointer to a `CHealthCheckConfig` struct.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn healthcheck_new(config: *const CHealthCheckConfig) -> *mut HealthCheckHandle {
    if config.is_null() {
        tracing::error!("healthcheck_new: null config");
        return std::ptr::null_mut();
    }

    let result = unsafe {
        let c_config = &*config;

        let (checker, target) = match create_checker(c_config) {
            Ok(v) => v,
            Err(_) => {
                tracing::error!("Failed to create checker");
                return std::ptr::null_mut();
            }
        };

        let check_config = HealthCheckConfig {
            target,
            timeout: Duration::from_millis(c_config.timeout_ms),
            interval: Duration::from_millis(c_config.interval_ms),
            rise: c_config.rise,
            fall: c_config.fall,
            check_type: CheckType::Tcp,
        };

        let runtime = match Runtime::new() {
            Ok(r) => Arc::new(r),
            Err(e) => {
                tracing::error!(error = %e, "Failed to create runtime");
                return std::ptr::null_mut();
            }
        };

        let monitor = HealthCheckMonitor::new(checker, check_config);

        HealthCheckHandle { monitor, runtime }
    };

    Box::into_raw(Box::new(result))
}

/// Free a health check monitor
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `healthcheck_new`, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn healthcheck_free(handle: *mut HealthCheckHandle) {
    if !handle.is_null() {
        unsafe {
            let handle = Box::from_raw(handle);
            // Stop the monitor before dropping
            handle.runtime.block_on(async {
                handle.monitor.stop().await;
            });
            drop(handle);
        }
    }
}

/// Start health checking
///
/// Returns 0 on success, -1 on error
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `healthcheck_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn healthcheck_start(handle: *mut HealthCheckHandle) -> i32 {
    if handle.is_null() {
        return -1;
    }

    unsafe {
        let handle = &*handle;
        handle.runtime.block_on(async {
            handle.monitor.start().await;
        });
    }

    0
}

/// Stop health checking
///
/// Returns 0 on success, -1 on error
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `healthcheck_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn healthcheck_stop(handle: *mut HealthCheckHandle) -> i32 {
    if handle.is_null() {
        return -1;
    }

    unsafe {
        let handle = &*handle;
        handle.runtime.block_on(async {
            handle.monitor.stop().await;
        });
    }

    0
}

/// Check if the service is healthy
///
/// Returns 1 if healthy, 0 if unhealthy, -1 on error
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `healthcheck_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn healthcheck_is_healthy(handle: *mut HealthCheckHandle) -> i32 {
    if handle.is_null() {
        return -1;
    }

    unsafe {
        let handle = &*handle;
        let is_healthy = handle
            .runtime
            .block_on(async { handle.monitor.is_healthy().await });

        if is_healthy { 1 } else { 0 }
    }
}

/// Get health check statistics
///
/// Returns 0 on success, -1 on error
///
/// # Safety
///
/// `handle` must be a valid pointer returned by `healthcheck_new`.
/// `stats` must be a valid pointer to a `CHealthCheckStats` struct.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn healthcheck_get_stats(
    handle: *mut HealthCheckHandle,
    stats: *mut CHealthCheckStats,
) -> i32 {
    if handle.is_null() || stats.is_null() {
        return -1;
    }

    unsafe {
        let handle = &*handle;
        let health_stats = handle
            .runtime
            .block_on(async { handle.monitor.get_stats().await });

        *stats = health_stats.into();
    }

    0
}

/// C-compatible health check result for one-shot checks
#[repr(C)]
pub struct CHealthCheckResult {
    pub status: CHealthStatus,
    pub duration_ms: u64,
    pub response_code: u16, // 0 if not applicable
}

/// Perform a one-shot health check (without monitor)
///
/// This is more efficient for single checks. Returns 0 on success, -1 on error.
///
/// # Safety
///
/// `config` must be a valid pointer to a `CHealthCheckConfig` struct.
/// `result` must be a valid pointer to a `CHealthCheckResult` struct.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn healthcheck_check_once(
    config: *const CHealthCheckConfig,
    result: *mut CHealthCheckResult,
) -> i32 {
    if config.is_null() || result.is_null() {
        return -1;
    }

    unsafe {
        let c_config = &*config;

        let (checker, _target) = match create_checker(c_config) {
            Ok(v) => v,
            Err(code) => return code,
        };

        let runtime = match Runtime::new() {
            Ok(r) => r,
            Err(_) => return -1,
        };

        let check_result = runtime.block_on(async { checker.check().await });

        *result = CHealthCheckResult {
            status: check_result.status.into(),
            duration_ms: check_result.duration.as_millis() as u64,
            response_code: check_result.response_code.unwrap_or(0),
        };

        0
    }
}

/// Get last error message
///
/// Returns a static string describing the last error, or NULL if no error.
/// The returned string should not be freed.
#[unsafe(no_mangle)]
pub extern "C" fn healthcheck_last_error() -> *const c_char {
    // TODO: Implement thread-local error storage
    std::ptr::null()
}
