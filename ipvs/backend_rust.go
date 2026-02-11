//go:build rust_ipvs
// +build rust_ipvs

package ipvs

import (
	"fmt"

	"github.com/google/seesaw/ipvs/rust"
)

// Rust backend implementation

type rustBackend struct {
	manager *rust.Manager
}

func newBackend() (Backend, error) {
	mgr, err := rust.NewManager()
	if err != nil {
		return nil, fmt.Errorf("failed to create Rust IPVS manager: %v", err)
	}
	return &rustBackend{manager: mgr}, nil
}

func (b *rustBackend) Init() error {
	// Manager already initialized in newBackend
	return nil
}

func (b *rustBackend) Exit() {
	if b.manager != nil {
		b.manager.Close()
		b.manager = nil
	}
}

func (b *rustBackend) Flush() error {
	return b.manager.Flush()
}

func (b *rustBackend) AddService(svc Service) error {
	rsvc := b.convertService(&svc)
	return b.manager.AddService(rsvc)
}

func (b *rustBackend) UpdateService(svc Service) error {
	rsvc := b.convertService(&svc)
	return b.manager.UpdateService(rsvc)
}

func (b *rustBackend) DeleteService(svc Service) error {
	rsvc := b.convertService(&svc)
	return b.manager.DeleteService(rsvc)
}

func (b *rustBackend) AddDestination(svc Service, dst Destination) error {
	rsvc := b.convertService(&svc)
	rdst := b.convertDestination(&dst)
	return b.manager.AddDestination(rsvc, rdst)
}

func (b *rustBackend) UpdateDestination(svc Service, dst Destination) error {
	rsvc := b.convertService(&svc)
	rdst := b.convertDestination(&dst)
	return b.manager.UpdateDestination(rsvc, rdst)
}

func (b *rustBackend) DeleteDestination(svc Service, dst Destination) error {
	rsvc := b.convertService(&svc)
	rdst := b.convertDestination(&dst)
	return b.manager.DeleteDestination(rsvc, rdst)
}

// Helper functions to convert between IPVS types and Rust types

func (b *rustBackend) convertService(svc *Service) *rust.Service {
	// Convert protocol from IPProto to uint8
	protocol := uint8(svc.Protocol)

	return &rust.Service{
		Address:   svc.Address,
		Protocol:  protocol,
		Port:      svc.Port,
		FWMark:    svc.FirewallMark,
		Scheduler: svc.Scheduler,
		Flags:     uint32(svc.Flags),
		Timeout:   svc.Timeout,
	}
}

func (b *rustBackend) convertDestination(dst *Destination) *rust.Destination {
	// Extract forwarding method from flags
	var fwdMethod uint8
	flags := dst.Flags
	switch {
	case flags&uint32(DFForwardMasq) != 0:
		fwdMethod = 0 // Masq
	case flags&uint32(DFForwardLocal) != 0:
		fwdMethod = 1 // Local
	case flags&uint32(DFForwardTunnel) != 0:
		fwdMethod = 2 // Tunnel
	case flags&uint32(DFForwardRoute) != 0:
		fwdMethod = 3 // Route
	case flags&uint32(DFForwardBypass) != 0:
		fwdMethod = 4 // Bypass
	default:
		fwdMethod = 0 // Default to masq
	}

	return &rust.Destination{
		Address:        dst.Address,
		Port:           dst.Port,
		Weight:         uint32(dst.Weight),
		ForwardMethod:  fwdMethod,
		LowerThreshold: dst.LowerThreshold,
		UpperThreshold: dst.UpperThreshold,
	}
}
