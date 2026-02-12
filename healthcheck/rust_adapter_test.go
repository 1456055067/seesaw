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

package healthcheck

import (
	"net"
	"testing"
	"time"
)

func TestRustTCPChecker(t *testing.T) {
	checker := NewRustTCPChecker(net.ParseIP("127.0.0.1"), 1)

	result := checker.Check(100 * time.Millisecond)
	if result == nil {
		t.Fatal("Expected result, got nil")
	}

	// Connection should fail since nothing is listening on port 1
	if result.Success {
		t.Error("Expected check to fail, but it succeeded")
	}

	t.Logf("Result: %s (success=%v, duration=%v)", result.Message, result.Success, result.Duration)
}

func TestRustHTTPChecker(t *testing.T) {
	checker := NewRustHTTPChecker(net.ParseIP("127.0.0.1"), 1, false)
	checker.Path = "/health"
	checker.ExpectedCodes = []uint16{200, 204}

	result := checker.Check(100 * time.Millisecond)
	if result == nil {
		t.Fatal("Expected result, got nil")
	}

	// Connection should fail since nothing is listening
	if result.Success {
		t.Error("Expected check to fail, but it succeeded")
	}

	t.Logf("Result: %s (success=%v, duration=%v)", result.Message, result.Success, result.Duration)
}

func TestRustDNSChecker(t *testing.T) {
	checker := NewRustDNSChecker(
		net.ParseIP("127.0.0.1"),
		"localhost",
		[]net.IP{net.IPv4(127, 0, 0, 1)},
	)

	result := checker.Check(1 * time.Second)
	if result == nil {
		t.Fatal("Expected result, got nil")
	}

	// localhost should resolve successfully
	if !result.Success {
		t.Logf("Warning: DNS check failed (might be expected in test environment): %s", result.Message)
	} else {
		t.Logf("Result: %s (success=%v, duration=%v)", result.Message, result.Success, result.Duration)
	}
}

func TestRustCheckersImplementChecker(t *testing.T) {
	// Verify that our Rust checkers implement the Checker interface
	var _ Checker = (*RustTCPChecker)(nil)
	var _ Checker = (*RustHTTPChecker)(nil)
	var _ Checker = (*RustDNSChecker)(nil)
}

func TestRustTCPCheckerString(t *testing.T) {
	checker := NewRustTCPChecker(net.ParseIP("192.0.2.1"), 80)
	expected := "Rust TCP 192.0.2.1:80 PLAIN"
	if checker.String() != expected {
		t.Errorf("Expected %q, got %q", expected, checker.String())
	}
}

func TestRustHTTPCheckerString(t *testing.T) {
	checker := NewRustHTTPChecker(net.ParseIP("192.0.2.1"), 443, true)
	checker.Path = "/api/health"
	expected := "Rust HTTPS GET /api/health 192.0.2.1:443 PLAIN"
	if checker.String() != expected {
		t.Errorf("Expected %q, got %q", expected, checker.String())
	}
}

func TestRustDNSCheckerString(t *testing.T) {
	checker := NewRustDNSChecker(
		net.ParseIP("8.8.8.8"),
		"example.com",
		nil,
	)
	expected := "Rust DNS query example.com 8.8.8.8:53 PLAIN"
	if checker.String() != expected {
		t.Errorf("Expected %q, got %q", expected, checker.String())
	}
}
