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

import (
	"net"
	"testing"
	"time"
)

func TestTCPMonitor(t *testing.T) {
	config := Config{
		Target:    "127.0.0.1:1", // Unlikely to be listening
		Timeout:   100 * time.Millisecond,
		Interval:  200 * time.Millisecond,
		Rise:      2,
		Fall:      2,
		CheckType: CheckTypeTCP,
	}

	monitor, err := NewMonitor(config)
	if err != nil {
		t.Fatalf("Failed to create monitor: %v", err)
	}
	defer monitor.Close()

	if err := monitor.Start(); err != nil {
		t.Fatalf("Failed to start monitor: %v", err)
	}
	defer monitor.Stop()

	// Wait for a few checks
	time.Sleep(500 * time.Millisecond)

	// Should be unhealthy since nothing is listening
	healthy, err := monitor.IsHealthy()
	if err != nil {
		t.Fatalf("Failed to check health: %v", err)
	}

	t.Logf("Service is healthy: %v", healthy)

	// Get statistics
	stats, err := monitor.GetStats()
	if err != nil {
		t.Fatalf("Failed to get stats: %v", err)
	}

	if stats.TotalChecks == 0 {
		t.Error("Expected at least one health check")
	}

	t.Logf("Stats: %+v", stats)
}

func TestHTTPMonitor(t *testing.T) {
	config := Config{
		Target:            "localhost:1",
		Timeout:           100 * time.Millisecond,
		Interval:          200 * time.Millisecond,
		Rise:              2,
		Fall:              2,
		CheckType:         CheckTypeHTTP,
		HTTPMethod:        "GET",
		HTTPPath:          "/health",
		HTTPExpectedCodes: []uint16{200, 204},
		HTTPUseHTTPS:      false,
	}

	monitor, err := NewMonitor(config)
	if err != nil {
		t.Fatalf("Failed to create monitor: %v", err)
	}
	defer monitor.Close()

	if err := monitor.Start(); err != nil {
		t.Fatalf("Failed to start monitor: %v", err)
	}
	defer monitor.Stop()

	// Wait for a few checks
	time.Sleep(500 * time.Millisecond)

	// Get statistics
	stats, err := monitor.GetStats()
	if err != nil {
		t.Fatalf("Failed to get stats: %v", err)
	}

	if stats.TotalChecks == 0 {
		t.Error("Expected at least one health check")
	}

	t.Logf("Stats: %+v", stats)
}

func TestDNSMonitor(t *testing.T) {
	config := Config{
		Target:         "localhost",
		Timeout:        1 * time.Second,
		Interval:       2 * time.Second,
		Rise:           2,
		Fall:           2,
		CheckType:      CheckTypeDNS,
		DNSQuery:       "localhost",
		DNSExpectedIPs: []net.IP{net.IPv4(127, 0, 0, 1)},
	}

	monitor, err := NewMonitor(config)
	if err != nil {
		t.Fatalf("Failed to create monitor: %v", err)
	}
	defer monitor.Close()

	if err := monitor.Start(); err != nil {
		t.Fatalf("Failed to start monitor: %v", err)
	}
	defer monitor.Stop()

	// Wait for a few checks
	time.Sleep(3 * time.Second)

	// localhost should resolve
	healthy, err := monitor.IsHealthy()
	if err != nil {
		t.Fatalf("Failed to check health: %v", err)
	}

	t.Logf("Service is healthy: %v", healthy)

	// Get statistics
	stats, err := monitor.GetStats()
	if err != nil {
		t.Fatalf("Failed to get stats: %v", err)
	}

	if stats.TotalChecks == 0 {
		t.Error("Expected at least one health check")
	}

	t.Logf("Stats: %+v", stats)
}
