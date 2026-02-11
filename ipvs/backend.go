// Package ipvs provides an interface to IPVS (IP Virtual Server).
//
// This package supports two backend implementations:
// 1. libnl (default) - Original C-based implementation
// 2. rust - New pure-Rust implementation (experimental)
//
// To use the Rust backend, build with: -tags rust_ipvs
package ipvs

import (
)

// Backend defines the interface for IPVS implementations.
// Note: Service and Destination types are defined in ipvs.go
type Backend interface {
	// Init initializes the IPVS backend
	Init() error

	// Exit cleans up the IPVS backend
	Exit()

	// Flush removes all IPVS services and destinations
	Flush() error

	// AddService adds a virtual service
	AddService(Service) error

	// UpdateService updates a virtual service
	UpdateService(Service) error

	// DeleteService removes a virtual service
	DeleteService(Service) error

	// AddDestination adds a destination to a service
	AddDestination(Service, Destination) error

	// UpdateDestination updates a destination
	UpdateDestination(Service, Destination) error

	// DeleteDestination removes a destination
	DeleteDestination(Service, Destination) error
}

// NewBackend creates the appropriate IPVS backend based on build tags.
//
// By default, uses the libnl-based C implementation.
// Build with -tags rust_ipvs to use the Rust implementation.
func NewBackend() (Backend, error) {
	return newBackend()
}
