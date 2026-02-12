#ifndef HEALTHCHECK_H
#define HEALTHCHECK_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Opaque handle to a health check monitor
 */
typedef struct HealthCheckHandle HealthCheckHandle;

/**
 * C-compatible health check configuration
 */
typedef struct CHealthCheckConfig {
  /**
   * Target address (IP:port or hostname:port)
   */
  const char *target;
  /**
   * Timeout in milliseconds
   */
  uint64_t timeout_ms;
  /**
   * Interval between checks in milliseconds
   */
  uint64_t interval_ms;
  /**
   * Number of consecutive successes required
   */
  uint32_t rise;
  /**
   * Number of consecutive failures required
   */
  uint32_t fall;
  /**
   * Check type (see CHealthCheckType)
   */
  uint8_t check_type;
  /**
   * HTTP method (for HTTP checks, e.g., "GET", "POST")
   */
  const char *http_method;
  /**
   * HTTP path (for HTTP checks, e.g., "/health")
   */
  const char *http_path;
  /**
   * Expected HTTP status codes (for HTTP checks)
   */
  const uint16_t *http_expected_codes;
  /**
   * Number of expected HTTP status codes
   */
  uintptr_t http_expected_codes_count;
  /**
   * Use HTTPS (for HTTP checks)
   */
  bool http_use_https;
  /**
   * DNS query name (for DNS checks)
   */
  const char *dns_query;
  /**
   * Expected IP addresses (for DNS checks)
   */
  const char *const *dns_expected_ips;
  /**
   * Number of expected IPs
   */
  uintptr_t dns_expected_ips_count;
} CHealthCheckConfig;

/**
 * C-compatible health check statistics
 */
typedef struct CHealthCheckStats {
  uint64_t total_checks;
  uint64_t successful_checks;
  uint64_t failed_checks;
  uint64_t timeouts;
  double avg_response_time_ms;
  uint32_t consecutive_successes;
  uint32_t consecutive_failures;
} CHealthCheckStats;

/**
 * Create a new health check monitor
 *
 * Returns NULL on error. Use healthcheck_free to clean up.
 */
struct HealthCheckHandle *healthcheck_new(const struct CHealthCheckConfig *config);

/**
 * Free a health check monitor
 */
void healthcheck_free(struct HealthCheckHandle *handle);

/**
 * Start health checking
 *
 * Returns 0 on success, -1 on error
 */
int32_t healthcheck_start(struct HealthCheckHandle *handle);

/**
 * Stop health checking
 *
 * Returns 0 on success, -1 on error
 */
int32_t healthcheck_stop(struct HealthCheckHandle *handle);

/**
 * Check if the service is healthy
 *
 * Returns 1 if healthy, 0 if unhealthy, -1 on error
 */
int32_t healthcheck_is_healthy(struct HealthCheckHandle *handle);

/**
 * Get health check statistics
 *
 * Returns 0 on success, -1 on error
 */
int32_t healthcheck_get_stats(struct HealthCheckHandle *handle, struct CHealthCheckStats *stats);

/**
 * Get last error message
 *
 * Returns a static string describing the last error, or NULL if no error.
 * The returned string should not be freed.
 */
const char *healthcheck_last_error(void);

#endif  /* HEALTHCHECK_H */
