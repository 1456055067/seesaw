//go:build !rust_ipvs
// +build !rust_ipvs

package ipvs

import "C"

// Default: use the existing libnl-based implementation

type libnlBackend struct {
	// Embeds the existing IPVS implementation
}

func newBackend() (Backend, error) {
	// Initialize existing IPVS implementation
	if err := Init(); err != nil {
		return nil, err
	}
	return &libnlBackend{}, nil
}

func (b *libnlBackend) Init() error {
	// Already initialized in newBackend
	return nil
}

func (b *libnlBackend) Exit() {
	Exit()
}

func (b *libnlBackend) Flush() error {
	return Flush()
}

func (b *libnlBackend) AddService(svc *Service) error {
	return AddService(svc)
}

func (b *libnlBackend) UpdateService(svc *Service) error {
	return UpdateService(svc)
}

func (b *libnlBackend) DeleteService(svc *Service) error {
	return DeleteService(svc)
}

func (b *libnlBackend) AddDestination(svc *Service, dst *Destination) error {
	return AddDestination(svc, dst)
}

func (b *libnlBackend) UpdateDestination(svc *Service, dst *Destination) error {
	return UpdateDestination(svc, dst)
}

func (b *libnlBackend) DeleteDestination(svc *Service, dst *Destination) error {
	return DeleteDestination(svc, dst)
}
