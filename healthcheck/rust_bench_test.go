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

// Benchmark Rust-backed TCP checker
func BenchmarkRustTCPChecker(b *testing.B) {
	checker := NewRustTCPChecker(net.ParseIP("127.0.0.1"), 1)
	timeout := 100 * time.Millisecond

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = checker.Check(timeout)
	}
}

// Benchmark Rust-backed HTTP checker
func BenchmarkRustHTTPChecker(b *testing.B) {
	checker := NewRustHTTPChecker(net.ParseIP("127.0.0.1"), 1, false)
	checker.Path = "/health"
	timeout := 100 * time.Millisecond

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = checker.Check(timeout)
	}
}

// Direct comparison benchmarks
func BenchmarkTCPComparison(b *testing.B) {
	timeout := 100 * time.Millisecond

	b.Run("Go", func(b *testing.B) {
		checker := NewTCPChecker(net.ParseIP("127.0.0.1"), 1)
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = checker.Check(timeout)
		}
	})

	b.Run("Rust", func(b *testing.B) {
		checker := NewRustTCPChecker(net.ParseIP("127.0.0.1"), 1)
		b.ResetTimer()
		for i := 0; i < b.N; i++ {
			_ = checker.Check(timeout)
		}
	})
}
