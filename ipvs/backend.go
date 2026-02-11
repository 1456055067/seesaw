// Package ipvs provides an interface to IPVS (IP Virtual Server).
//
// This package supports two backend implementations:
// 1. libnl (default) - Original C-based implementation
// 2. rust - New pure-Rust implementation (experimental)
//
// To use the Rust backend, build with: -tags rust_ipvs
package ipvs

import (
	"fmt"
	"net"
)

// Backend defines the interface for IPVS implementations.
type Backend interface {
	// Init initializes the IPVS backend
	Init() error

	// Exit cleans up the IPVS backend
	Exit()

	// Flush removes all IPVS services and destinations
	Flush() error

	// AddService adds a virtual service
	AddService(*Service) error

	// UpdateService updates a virtual service
	UpdateService(*Service) error

	// DeleteService removes a virtual service
	DeleteService(*Service) error

	// AddDestination adds a destination to a service
	AddDestination(*Service, *Destination) error

	// UpdateDestination updates a destination
	UpdateDestination(*Service, *Destination) error

	// DeleteDestination removes a destination
	DeleteDestination(*Service, *Destination) error
}

// Version represents the IPVS version
type Version struct {
	Major uint32
	Minor uint32
	Patch uint32
}

func (v Version) String() string {
	return fmt.Sprintf("%d.%d.%d", v.Major, v.Minor, v.Patch)
}

// Service represents an IPVS virtual service (shared between backends)
type Service struct {
	Address       net.IP
	Protocol      uint16
	Port          uint16
	Scheduler     string
	Flags         uint32
	Timeout       uint32
	FirewallMark  uint32
	AddressFamily uint16
	Netmask       uint32
}

// Destination represents an IPVS destination (shared between backends)
type Destination struct {
	Address        net.IP
	Port           uint16
	Weight         uint32
	FwdMethod      uint32
	AddressFamily  uint16
	UpperThreshold uint32
	LowerThreshold uint32
}

// NewBackend creates the appropriate IPVS backend based on build tags.
//
// By default, uses the libnl-based C implementation.
// Build with -tags rust_ipvs to use the Rust implementation.
func NewBackend() (Backend, error) {
	return newBackend()
}
