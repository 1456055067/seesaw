// Copyright 2024 Google Inc. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//go:build rust_healthcheck

package rust

// #cgo LDFLAGS: -L${SRCDIR}/../../rust/target/release -lhealthcheck_ffi -lpthread -ldl -lm
// #include "../../rust/crates/healthcheck-ffi/healthcheck.h"
// #include <stdlib.h>
import "C"

import (
	"fmt"
	"net"
	"time"
	"unsafe"
)

// CheckType represents the type of health check
type CheckType uint8

const (
	CheckTypeTCP  CheckType = 0
	CheckTypeHTTP CheckType = 1
	CheckTypePing CheckType = 2
	CheckTypeDNS  CheckType = 3
)

// HealthStatus represents the health status of a check
type HealthStatus uint8

const (
	HealthStatusHealthy   HealthStatus = 0
	HealthStatusUnhealthy HealthStatus = 1
	HealthStatusTimeout   HealthStatus = 2
	HealthStatusError     HealthStatus = 3
)

// Config represents health check configuration
type Config struct {
	// Target address (IP:port or hostname:port)
	Target string

	// Timeout for the health check
	Timeout time.Duration

	// Interval between checks
	Interval time.Duration

	// Number of consecutive successes required
	Rise uint32

	// Number of consecutive failures required
	Fall uint32

	// Check type
	CheckType CheckType

	// HTTP-specific configuration
	HTTPMethod        string   // e.g., "GET", "POST"
	HTTPPath          string   // e.g., "/health"
	HTTPExpectedCodes []uint16 // Expected status codes
	HTTPUseHTTPS      bool

	// DNS-specific configuration
	DNSQuery       string   // Query name
	DNSExpectedIPs []net.IP // Expected IP addresses
}

// Stats represents health check statistics
type Stats struct {
	TotalChecks          uint64
	SuccessfulChecks     uint64
	FailedChecks         uint64
	Timeouts             uint64
	AvgResponseTimeMs    float64
	ConsecutiveSuccesses uint32
	ConsecutiveFailures  uint32
}

// Monitor wraps a Rust health check monitor
type Monitor struct {
	handle *C.HealthCheckHandle
}

// NewMonitor creates a new health check monitor
func NewMonitor(config Config) (*Monitor, error) {
	// Convert target to C string
	cTarget := C.CString(config.Target)
	defer C.free(unsafe.Pointer(cTarget))

	// Build C configuration
	cConfig := C.CHealthCheckConfig{
		target:      cTarget,
		timeout_ms:  C.uint64_t(config.Timeout.Milliseconds()),
		interval_ms: C.uint64_t(config.Interval.Milliseconds()),
		rise:        C.uint32_t(config.Rise),
		fall:        C.uint32_t(config.Fall),
		check_type:  C.uint8_t(config.CheckType),
	}

	// Add type-specific configuration
	switch config.CheckType {
	case CheckTypeHTTP:
		// HTTP method
		if config.HTTPMethod == "" {
			config.HTTPMethod = "GET"
		}
		cMethod := C.CString(config.HTTPMethod)
		defer C.free(unsafe.Pointer(cMethod))
		cConfig.http_method = cMethod

		// HTTP path
		if config.HTTPPath == "" {
			config.HTTPPath = "/"
		}
		cPath := C.CString(config.HTTPPath)
		defer C.free(unsafe.Pointer(cPath))
		cConfig.http_path = cPath

		// Expected codes
		if len(config.HTTPExpectedCodes) > 0 {
			cCodes := C.malloc(C.size_t(len(config.HTTPExpectedCodes)) * C.size_t(unsafe.Sizeof(uint16(0))))
			defer C.free(cCodes)

			codesSlice := (*[1 << 30]C.uint16_t)(cCodes)[:len(config.HTTPExpectedCodes):len(config.HTTPExpectedCodes)]
			for i, code := range config.HTTPExpectedCodes {
				codesSlice[i] = C.uint16_t(code)
			}

			cConfig.http_expected_codes = (*C.uint16_t)(cCodes)
			cConfig.http_expected_codes_count = C.uintptr_t(len(config.HTTPExpectedCodes))
		}

		// HTTPS flag
		cConfig.http_use_https = C.bool(config.HTTPUseHTTPS)

	case CheckTypeDNS:
		// DNS query
		cQuery := C.CString(config.DNSQuery)
		defer C.free(unsafe.Pointer(cQuery))
		cConfig.dns_query = cQuery

		// Expected IPs
		if len(config.DNSExpectedIPs) > 0 {
			// Allocate array of char pointers
			cIPsArray := C.malloc(C.size_t(len(config.DNSExpectedIPs)) * C.size_t(unsafe.Sizeof(uintptr(0))))
			defer C.free(cIPsArray)

			ipsSlice := (*[1 << 30]*C.char)(cIPsArray)[:len(config.DNSExpectedIPs):len(config.DNSExpectedIPs)]

			// Allocate individual IP strings
			cIPStrings := make([]*C.char, len(config.DNSExpectedIPs))
			for i, ip := range config.DNSExpectedIPs {
				cIPStrings[i] = C.CString(ip.String())
				ipsSlice[i] = cIPStrings[i]
			}

			// Clean up IP strings after the C function call
			defer func() {
				for _, cIP := range cIPStrings {
					C.free(unsafe.Pointer(cIP))
				}
			}()

			cConfig.dns_expected_ips = (**C.char)(cIPsArray)
			cConfig.dns_expected_ips_count = C.uintptr_t(len(config.DNSExpectedIPs))
		}
	}

	// Create monitor
	handle := C.healthcheck_new(&cConfig)
	if handle == nil {
		return nil, fmt.Errorf("failed to create health check monitor")
	}

	return &Monitor{handle: handle}, nil
}

// Start begins health checking
func (m *Monitor) Start() error {
	result := C.healthcheck_start(m.handle)
	if result != 0 {
		return fmt.Errorf("failed to start health check monitor")
	}
	return nil
}

// Stop stops health checking
func (m *Monitor) Stop() error {
	result := C.healthcheck_stop(m.handle)
	if result != 0 {
		return fmt.Errorf("failed to stop health check monitor")
	}
	return nil
}

// IsHealthy returns whether the service is currently healthy
func (m *Monitor) IsHealthy() (bool, error) {
	result := C.healthcheck_is_healthy(m.handle)
	if result == -1 {
		return false, fmt.Errorf("failed to check health status")
	}
	return result == 1, nil
}

// GetStats returns health check statistics
func (m *Monitor) GetStats() (*Stats, error) {
	var cStats C.CHealthCheckStats
	result := C.healthcheck_get_stats(m.handle, &cStats)
	if result != 0 {
		return nil, fmt.Errorf("failed to get health check stats")
	}

	return &Stats{
		TotalChecks:          uint64(cStats.total_checks),
		SuccessfulChecks:     uint64(cStats.successful_checks),
		FailedChecks:         uint64(cStats.failed_checks),
		Timeouts:             uint64(cStats.timeouts),
		AvgResponseTimeMs:    float64(cStats.avg_response_time_ms),
		ConsecutiveSuccesses: uint32(cStats.consecutive_successes),
		ConsecutiveFailures:  uint32(cStats.consecutive_failures),
	}, nil
}

// Close frees the health check monitor
func (m *Monitor) Close() {
	if m.handle != nil {
		C.healthcheck_free(m.handle)
		m.handle = nil
	}
}
