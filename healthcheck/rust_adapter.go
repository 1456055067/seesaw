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

// Rust-backed healthcheck adapter for Seesaw.
//
// This package provides adapters that wrap Rust health checkers and implement
// the standard Seesaw Checker interface, allowing Rust-backed checkers to be
// used as drop-in replacements for Go checkers.

package healthcheck

// #cgo LDFLAGS: -L${SRCDIR}/../rust/target/release -lhealthcheck_ffi -lpthread -ldl -lm
// #include "../rust/crates/healthcheck-ffi/healthcheck.h"
// #include <stdlib.h>
import "C"

import (
	"fmt"
	"net"
	"time"
	"unsafe"
)

// RustTCPChecker is a Rust-backed TCP healthchecker.
type RustTCPChecker struct {
	Target
}

// NewRustTCPChecker returns an initialised Rust-backed TCP checker.
func NewRustTCPChecker(ip net.IP, port int) *RustTCPChecker {
	return &RustTCPChecker{
		Target: Target{
			IP:    ip,
			Port:  port,
			Proto: 6, // TCP
		},
	}
}

// Check performs a TCP healthcheck using the Rust implementation.
func (hc *RustTCPChecker) Check(timeout time.Duration) *Result {
	start := time.Now()

	// Convert target to C string
	cTarget := C.CString(hc.Target.addr())
	defer C.free(unsafe.Pointer(cTarget))

	// Build C configuration
	cConfig := C.CHealthCheckConfig{
		target:      cTarget,
		timeout_ms:  C.uint64_t(timeout.Milliseconds()),
		interval_ms: 0, // Not used for one-shot checks
		rise:        1,
		fall:        1,
		check_type:  0, // TCP
	}

	// Perform the check
	var cResult C.CHealthCheckResult
	ret := C.healthcheck_check_once(&cConfig, &cResult)
	if ret != 0 {
		return complete(start, "Failed to perform health check", false, fmt.Errorf("healthcheck_check_once failed"))
	}

	duration := time.Duration(cResult.duration_ms) * time.Millisecond
	success := cResult.status == C.Healthy

	var message string
	if success {
		message = fmt.Sprintf("TCP connection successful (%v)", duration)
	} else {
		message = fmt.Sprintf("TCP connection failed: %s", healthStatusString(cResult.status))
	}

	return &Result{
		Message:  message,
		Success:  success,
		Duration: duration,
		Err:      nil,
	}
}

// String returns the string representation of this healthcheck.
func (hc *RustTCPChecker) String() string {
	return fmt.Sprintf("Rust TCP %s", hc.Target.String())
}

// RustHTTPChecker is a Rust-backed HTTP/HTTPS healthchecker.
type RustHTTPChecker struct {
	Target
	Method        string
	Path          string
	ExpectedCodes []uint16
	Secure        bool
}

// NewRustHTTPChecker returns an initialised Rust-backed HTTP checker.
func NewRustHTTPChecker(ip net.IP, port int, secure bool) *RustHTTPChecker {
	return &RustHTTPChecker{
		Target: Target{
			IP:    ip,
			Port:  port,
			Proto: 6, // TCP
		},
		Method:        "GET",
		Path:          "/",
		ExpectedCodes: []uint16{200},
		Secure:        secure,
	}
}

// Check performs an HTTP healthcheck using the Rust implementation.
func (hc *RustHTTPChecker) Check(timeout time.Duration) *Result {
	start := time.Now()

	// Convert target to C string
	cTarget := C.CString(hc.Target.addr())
	defer C.free(unsafe.Pointer(cTarget))

	// Convert method and path
	cMethod := C.CString(hc.Method)
	defer C.free(unsafe.Pointer(cMethod))

	cPath := C.CString(hc.Path)
	defer C.free(unsafe.Pointer(cPath))

	// Convert expected codes
	var cCodes *C.uint16_t
	if len(hc.ExpectedCodes) > 0 {
		cCodesArray := C.malloc(C.size_t(len(hc.ExpectedCodes)) * C.size_t(unsafe.Sizeof(uint16(0))))
		defer C.free(cCodesArray)

		codesSlice := (*[1 << 30]C.uint16_t)(cCodesArray)[:len(hc.ExpectedCodes):len(hc.ExpectedCodes)]
		for i, code := range hc.ExpectedCodes {
			codesSlice[i] = C.uint16_t(code)
		}
		cCodes = (*C.uint16_t)(cCodesArray)
	}

	// Build C configuration
	cConfig := C.CHealthCheckConfig{
		target:                    cTarget,
		timeout_ms:                C.uint64_t(timeout.Milliseconds()),
		interval_ms:               0, // Not used for one-shot checks
		rise:                      1,
		fall:                      1,
		check_type:                1, // HTTP
		http_method:               cMethod,
		http_path:                 cPath,
		http_expected_codes:       cCodes,
		http_expected_codes_count: C.uintptr_t(len(hc.ExpectedCodes)),
		http_use_https:            C.bool(hc.Secure),
	}

	// Perform the check
	var cResult C.CHealthCheckResult
	ret := C.healthcheck_check_once(&cConfig, &cResult)
	if ret != 0 {
		return complete(start, "Failed to perform health check", false, fmt.Errorf("healthcheck_check_once failed"))
	}

	duration := time.Duration(cResult.duration_ms) * time.Millisecond
	success := cResult.status == C.Healthy

	protocol := "HTTP"
	if hc.Secure {
		protocol = "HTTPS"
	}

	var message string
	if success {
		message = fmt.Sprintf("%s %s %s successful (status %d, %v)", protocol, hc.Method, hc.Path, cResult.response_code, duration)
	} else {
		message = fmt.Sprintf("%s request failed: %s", protocol, healthStatusString(cResult.status))
		if cResult.response_code > 0 {
			message += fmt.Sprintf(" (status %d)", cResult.response_code)
		}
	}

	return &Result{
		Message:  message,
		Success:  success,
		Duration: duration,
		Err:      nil,
	}
}

// String returns the string representation of this healthcheck.
func (hc *RustHTTPChecker) String() string {
	protocol := "HTTP"
	if hc.Secure {
		protocol = "HTTPS"
	}
	return fmt.Sprintf("Rust %s %s %s %s", protocol, hc.Method, hc.Path, hc.Target.String())
}

// RustDNSChecker is a Rust-backed DNS healthchecker.
type RustDNSChecker struct {
	Target
	Query       string
	ExpectedIPs []net.IP
}

// NewRustDNSChecker returns an initialised Rust-backed DNS checker.
func NewRustDNSChecker(ip net.IP, query string, expectedIPs []net.IP) *RustDNSChecker {
	return &RustDNSChecker{
		Target: Target{
			IP:    ip,
			Port:  53,
			Proto: 17, // UDP
		},
		Query:       query,
		ExpectedIPs: expectedIPs,
	}
}

// Check performs a DNS healthcheck using the Rust implementation.
func (hc *RustDNSChecker) Check(timeout time.Duration) *Result {
	start := time.Now()

	// Convert target to C string
	cTarget := C.CString(hc.Target.addr())
	defer C.free(unsafe.Pointer(cTarget))

	// Convert query
	cQuery := C.CString(hc.Query)
	defer C.free(unsafe.Pointer(cQuery))

	// Convert expected IPs
	var cIPsArray unsafe.Pointer
	var cIPStrings []*C.char
	if len(hc.ExpectedIPs) > 0 {
		cIPsArray = C.malloc(C.size_t(len(hc.ExpectedIPs)) * C.size_t(unsafe.Sizeof(uintptr(0))))
		defer C.free(cIPsArray)

		ipsSlice := (*[1 << 30]*C.char)(cIPsArray)[:len(hc.ExpectedIPs):len(hc.ExpectedIPs)]
		cIPStrings = make([]*C.char, len(hc.ExpectedIPs))
		for i, ip := range hc.ExpectedIPs {
			cIPStrings[i] = C.CString(ip.String())
			ipsSlice[i] = cIPStrings[i]
		}
		defer func() {
			for _, cIP := range cIPStrings {
				C.free(unsafe.Pointer(cIP))
			}
		}()
	}

	// Build C configuration
	cConfig := C.CHealthCheckConfig{
		target:                 cTarget,
		timeout_ms:             C.uint64_t(timeout.Milliseconds()),
		interval_ms:            0, // Not used for one-shot checks
		rise:                   1,
		fall:                   1,
		check_type:             3, // DNS
		dns_query:              cQuery,
		dns_expected_ips:       (**C.char)(cIPsArray),
		dns_expected_ips_count: C.uintptr_t(len(hc.ExpectedIPs)),
	}

	// Perform the check
	var cResult C.CHealthCheckResult
	ret := C.healthcheck_check_once(&cConfig, &cResult)
	if ret != 0 {
		return complete(start, "Failed to perform health check", false, fmt.Errorf("healthcheck_check_once failed"))
	}

	duration := time.Duration(cResult.duration_ms) * time.Millisecond
	success := cResult.status == C.Healthy

	var message string
	if success {
		message = fmt.Sprintf("DNS query for %s successful (%v)", hc.Query, duration)
	} else {
		message = fmt.Sprintf("DNS query for %s failed: %s", hc.Query, healthStatusString(cResult.status))
	}

	return &Result{
		Message:  message,
		Success:  success,
		Duration: duration,
		Err:      nil,
	}
}

// String returns the string representation of this healthcheck.
func (hc *RustDNSChecker) String() string {
	return fmt.Sprintf("Rust DNS query %s %s", hc.Query, hc.Target.String())
}

// healthStatusString converts a C health status to a string.
func healthStatusString(status uint32) string {
	switch C.CHealthStatus(status) {
	case C.Healthy:
		return "Healthy"
	case C.Unhealthy:
		return "Unhealthy"
	case C.Timeout:
		return "Timeout"
	case C.Error:
		return "Error"
	default:
		return "Unknown"
	}
}
