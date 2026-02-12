//! Health check implementations.

use crate::types::HealthCheckResult;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Health checker trait
#[async_trait]
pub trait HealthChecker: Send + Sync {
    /// Perform a health check
    async fn check(&self) -> HealthCheckResult;
    
    /// Get the name of this checker
    fn name(&self) -> &str;
}

/// TCP health checker
pub struct TcpChecker {
    target: SocketAddr,
    timeout_duration: Duration,
}

impl TcpChecker {
    /// Create a new TCP health checker
    pub fn new(target: SocketAddr, timeout_duration: Duration) -> Self {
        Self {
            target,
            timeout_duration,
        }
    }
}

#[async_trait]
impl HealthChecker for TcpChecker {
    async fn check(&self) -> HealthCheckResult {
        let start = Instant::now();
        
        match timeout(self.timeout_duration, TcpStream::connect(self.target)).await {
            Ok(Ok(_stream)) => {
                let duration = start.elapsed();
                debug!(target = %self.target, duration_ms = duration.as_millis(), "TCP check successful");
                HealthCheckResult::healthy(duration)
            }
            Ok(Err(e)) => {
                let duration = start.elapsed();
                warn!(target = %self.target, error = %e, "TCP check failed");
                HealthCheckResult::unhealthy(duration, format!("Connection failed: {}", e))
            }
            Err(_) => {
                let duration = start.elapsed();
                warn!(target = %self.target, "TCP check timed out");
                HealthCheckResult::timeout(duration)
            }
        }
    }
    
    fn name(&self) -> &str {
        "tcp"
    }
}

/// HTTP health checker
pub struct HttpChecker {
    url: String,
    method: reqwest::Method,
    expected_codes: Vec<u16>,
    timeout_duration: Duration,
    client: reqwest::Client,
}

impl HttpChecker {
    /// Create a new HTTP health checker
    pub fn new(
        url: String,
        method: reqwest::Method,
        expected_codes: Vec<u16>,
        timeout_duration: Duration,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder()
            .timeout(timeout_duration)
            .build()?;
        
        Ok(Self {
            url,
            method,
            expected_codes,
            timeout_duration,
            client,
        })
    }
}

#[async_trait]
impl HealthChecker for HttpChecker {
    async fn check(&self) -> HealthCheckResult {
        let start = Instant::now();
        
        let request = self.client.request(self.method.clone(), &self.url);
        
        match timeout(self.timeout_duration, request.send()).await {
            Ok(Ok(response)) => {
                let duration = start.elapsed();
                let status_code = response.status().as_u16();
                
                if self.expected_codes.is_empty() || self.expected_codes.contains(&status_code) {
                    debug!(url = %self.url, status = status_code, duration_ms = duration.as_millis(), 
                           "HTTP check successful");
                    let mut result = HealthCheckResult::healthy(duration);
                    result.response_code = Some(status_code);
                    result
                } else {
                    warn!(url = %self.url, status = status_code, "HTTP check failed: unexpected status code");
                    let mut result = HealthCheckResult::unhealthy(
                        duration,
                        format!("Unexpected status code: {}", status_code),
                    );
                    result.response_code = Some(status_code);
                    result
                }
            }
            Ok(Err(e)) => {
                let duration = start.elapsed();
                warn!(url = %self.url, error = %e, "HTTP check failed");
                HealthCheckResult::error(duration, format!("HTTP request failed: {}", e))
            }
            Err(_) => {
                let duration = start.elapsed();
                warn!(url = %self.url, "HTTP check timed out");
                HealthCheckResult::timeout(duration)
            }
        }
    }
    
    fn name(&self) -> &str {
        "http"
    }
}

/// DNS health checker
pub struct DnsChecker {
    query: String,
    expected_ips: Vec<std::net::IpAddr>,
    timeout_duration: Duration,
}

impl DnsChecker {
    /// Create a new DNS health checker
    pub fn new(
        query: String,
        expected_ips: Vec<std::net::IpAddr>,
        timeout_duration: Duration,
    ) -> Self {
        Self {
            query,
            expected_ips,
            timeout_duration,
        }
    }
}

#[async_trait]
impl HealthChecker for DnsChecker {
    async fn check(&self) -> HealthCheckResult {
        let start = Instant::now();
        
        // Use system DNS resolver
        match timeout(
            self.timeout_duration,
            tokio::net::lookup_host(format!("{}:0", self.query))
        ).await {
            Ok(Ok(addrs)) => {
                let duration = start.elapsed();
                let resolved_ips: Vec<std::net::IpAddr> = addrs.map(|addr| addr.ip()).collect();
                
                if self.expected_ips.is_empty() {
                    // Just check that resolution succeeded
                    if !resolved_ips.is_empty() {
                        debug!(query = %self.query, count = resolved_ips.len(), "DNS check successful");
                        HealthCheckResult::healthy(duration)
                    } else {
                        warn!(query = %self.query, "DNS check failed: no IPs resolved");
                        HealthCheckResult::unhealthy(duration, "No IPs resolved")
                    }
                } else {
                    // Check if any expected IP is in the results
                    let found = self.expected_ips.iter().any(|expected| resolved_ips.contains(expected));
                    
                    if found {
                        debug!(query = %self.query, "DNS check successful: expected IP found");
                        HealthCheckResult::healthy(duration)
                    } else {
                        warn!(query = %self.query, "DNS check failed: expected IP not found");
                        HealthCheckResult::unhealthy(duration, "Expected IP not found in DNS results")
                    }
                }
            }
            Ok(Err(e)) => {
                let duration = start.elapsed();
                warn!(query = %self.query, error = %e, "DNS check failed");
                HealthCheckResult::error(duration, format!("DNS lookup failed: {}", e))
            }
            Err(_) => {
                let duration = start.elapsed();
                warn!(query = %self.query, "DNS check timed out");
                HealthCheckResult::timeout(duration)
            }
        }
    }
    
    fn name(&self) -> &str {
        "dns"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HealthStatus;

    #[tokio::test]
    async fn test_tcp_checker_localhost() {
        // This will fail if nothing is listening, which is expected
        let checker = TcpChecker::new(
            "127.0.0.1:1".parse().unwrap(),
            Duration::from_millis(100),
        );
        
        let result = checker.check().await;
        assert!(result.duration <= Duration::from_millis(200));
    }

    #[tokio::test]
    async fn test_http_checker() {
        // This will fail without network, which is fine for unit tests
        let checker = HttpChecker::new(
            "http://localhost:1/health".to_string(),
            reqwest::Method::GET,
            vec![200],
            Duration::from_millis(100),
        ).unwrap();
        
        let result = checker.check().await;
        assert!(result.duration <= Duration::from_millis(200));
    }

    #[tokio::test]
    async fn test_dns_checker() {
        let checker = DnsChecker::new(
            "localhost".to_string(),
            vec![],
            Duration::from_secs(1),
        );
        
        let result = checker.check().await;
        // localhost should always resolve
        assert!(result.is_healthy() || matches!(result.status, HealthStatus::Error));
    }
}
