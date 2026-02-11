// Package rust provides Go bindings to the Rust IPVS implementation.
//
// This package uses CGo to call into the Rust IPVS library, providing
// a pure-Rust alternative to the current libnl-based implementation.
package rust

// #cgo LDFLAGS: -L${SRCDIR}/../../rust/target/release -lipvs_ffi
// #include "../../../rust/crates/ipvs-ffi/ipvs.h"
// #include <stdlib.h>
import "C"
import (
	"fmt"
	"net"
	"unsafe"
)

// Manager wraps the Rust IPVS manager.
type Manager struct {
	handle *C.IpvsHandle
}

// Version represents the IPVS kernel version.
type Version struct {
	Major uint32
	Minor uint32
	Patch uint32
}

func (v Version) String() string {
	return fmt.Sprintf("%d.%d.%d", v.Major, v.Minor, v.Patch)
}

// Service represents an IPVS virtual service.
type Service struct {
	Address   net.IP
	Protocol  uint8  // 6=TCP, 17=UDP, 132=SCTP
	Port      uint16
	FWMark    uint32
	Scheduler string
	Flags     uint32
	Timeout   uint32
}

// Destination represents an IPVS destination (backend server).
type Destination struct {
	Address        net.IP
	Port           uint16
	Weight         uint32
	ForwardMethod  uint8  // 0=Masq, 1=Local, 2=Tunnel, 3=Route, 4=Bypass
	LowerThreshold uint32
	UpperThreshold uint32
}

// NewManager creates a new IPVS manager instance.
func NewManager() (*Manager, error) {
	handle := C.ipvs_new()
	if handle == nil {
		return nil, fmt.Errorf("failed to create IPVS manager")
	}
	return &Manager{handle: handle}, nil
}

// Close destroys the IPVS manager and frees resources.
func (m *Manager) Close() {
	if m.handle != nil {
		C.ipvs_destroy(m.handle)
		m.handle = nil
	}
}

// Version returns the IPVS kernel version.
func (m *Manager) Version() (*Version, error) {
	var cver C.CVersion
	ret := C.ipvs_version(m.handle, &cver)
	if ret != 0 {
		return nil, m.makeError(ret, "version")
	}
	return &Version{
		Major: uint32(cver.major),
		Minor: uint32(cver.minor),
		Patch: uint32(cver.patch),
	}, nil
}

// Flush removes all IPVS services and destinations.
func (m *Manager) Flush() error {
	ret := C.ipvs_flush(m.handle)
	if ret != 0 {
		return m.makeError(ret, "flush")
	}
	return nil
}

// AddService adds a new virtual service.
func (m *Manager) AddService(svc *Service) error {
	csvc, err := m.serviceToC(svc)
	if err != nil {
		return err
	}
	defer C.free(unsafe.Pointer(csvc.scheduler))

	ret := C.ipvs_add_service(m.handle, csvc)
	if ret != 0 {
		return m.makeError(ret, "add_service")
	}
	return nil
}

// UpdateService updates an existing virtual service.
func (m *Manager) UpdateService(svc *Service) error {
	csvc, err := m.serviceToC(svc)
	if err != nil {
		return err
	}
	defer C.free(unsafe.Pointer(csvc.scheduler))

	ret := C.ipvs_update_service(m.handle, csvc)
	if ret != 0 {
		return m.makeError(ret, "update_service")
	}
	return nil
}

// DeleteService removes a virtual service.
func (m *Manager) DeleteService(svc *Service) error {
	csvc, err := m.serviceToC(svc)
	if err != nil {
		return err
	}
	defer C.free(unsafe.Pointer(csvc.scheduler))

	ret := C.ipvs_delete_service(m.handle, csvc)
	if ret != 0 {
		return m.makeError(ret, "delete_service")
	}
	return nil
}

// AddDestination adds a backend server to a service.
func (m *Manager) AddDestination(svc *Service, dest *Destination) error {
	csvc, err := m.serviceToC(svc)
	if err != nil {
		return err
	}
	defer C.free(unsafe.Pointer(csvc.scheduler))

	cdest, err := m.destToC(dest)
	if err != nil {
		return err
	}

	ret := C.ipvs_add_destination(m.handle, csvc, cdest)
	if ret != 0 {
		return m.makeError(ret, "add_destination")
	}
	return nil
}

// UpdateDestination updates a backend server in a service.
func (m *Manager) UpdateDestination(svc *Service, dest *Destination) error {
	csvc, err := m.serviceToC(svc)
	if err != nil {
		return err
	}
	defer C.free(unsafe.Pointer(csvc.scheduler))

	cdest, err := m.destToC(dest)
	if err != nil {
		return err
	}

	ret := C.ipvs_update_destination(m.handle, csvc, cdest)
	if ret != 0 {
		return m.makeError(ret, "update_destination")
	}
	return nil
}

// DeleteDestination removes a backend server from a service.
func (m *Manager) DeleteDestination(svc *Service, dest *Destination) error {
	csvc, err := m.serviceToC(svc)
	if err != nil {
		return err
	}
	defer C.free(unsafe.Pointer(csvc.scheduler))

	cdest, err := m.destToC(dest)
	if err != nil {
		return err
	}

	ret := C.ipvs_delete_destination(m.handle, csvc, cdest)
	if ret != 0 {
		return m.makeError(ret, "delete_destination")
	}
	return nil
}

// Helper functions

func (m *Manager) serviceToC(svc *Service) (*C.CService, error) {
	if len(svc.Address) != 4 {
		return nil, fmt.Errorf("only IPv4 addresses supported")
	}

	// Convert IP to uint32 in network byte order
	addr := uint32(svc.Address[0])<<24 |
		uint32(svc.Address[1])<<16 |
		uint32(svc.Address[2])<<8 |
		uint32(svc.Address[3])

	// Convert scheduler to C string
	scheduler := C.CString(svc.Scheduler)

	csvc := &C.CService{
		address:   C.uint32_t(addr),
		protocol:  C.uint8_t(svc.Protocol),
		port:      C.uint16_t(htons(svc.Port)),
		fwmark:    C.uint32_t(svc.FWMark),
		scheduler: scheduler,
		flags:     C.uint32_t(svc.Flags),
		timeout:   C.uint32_t(svc.Timeout),
	}

	return csvc, nil
}

func (m *Manager) destToC(dest *Destination) (*C.CDestination, error) {
	if len(dest.Address) != 4 {
		return nil, fmt.Errorf("only IPv4 addresses supported")
	}

	// Convert IP to uint32 in network byte order
	addr := uint32(dest.Address[0])<<24 |
		uint32(dest.Address[1])<<16 |
		uint32(dest.Address[2])<<8 |
		uint32(dest.Address[3])

	cdest := &C.CDestination{
		address:          C.uint32_t(addr),
		port:             C.uint16_t(htons(dest.Port)),
		weight:           C.uint32_t(dest.Weight),
		fwd_method:       C.uint8_t(dest.ForwardMethod),
		lower_threshold:  C.uint32_t(dest.LowerThreshold),
		upper_threshold:  C.uint32_t(dest.UpperThreshold),
	}

	return cdest, nil
}

func (m *Manager) makeError(code C.int, op string) error {
	errStr := C.GoString(C.ipvs_error_string(code))
	return fmt.Errorf("%s failed: %s (code %d)", op, errStr, int(code))
}

// htons converts host byte order to network byte order (big-endian)
func htons(v uint16) uint16 {
	return (v<<8)&0xff00 | (v>>8)&0x00ff
}
