//go:build rust_vrrp
// +build rust_vrrp

// Package rust provides Go bindings to the Rust VRRP implementation.
package rust

// #cgo LDFLAGS: -L${SRCDIR}/../../rust/target/release -lvrrp_ffi -lpthread -ldl -lm
// #include "../../rust/crates/vrrp-ffi/vrrp.h"
// #include <stdlib.h>
import "C"
import (
	"errors"
	"fmt"
	"net"
	"unsafe"
)

// Node represents a VRRP node instance.
type Node struct {
	handle *C.VrrpHandle
}

// Config represents VRRP configuration.
type Config struct {
	VRID            uint8
	Priority        uint8
	AdvertInterval  uint16
	Preempt         bool
	Interface       string
	PrimaryIP       net.IP
	VirtualIPs      []net.IP
}

// State represents the VRRP state.
type State int

const (
	// StateInit is the initial state.
	StateInit State = C.VRRP_STATE_INIT
	// StateBackup is the backup state.
	StateBackup State = C.VRRP_STATE_BACKUP
	// StateMaster is the master state.
	StateMaster State = C.VRRP_STATE_MASTER
)

// String returns the string representation of the state.
func (s State) String() string {
	switch s {
	case StateInit:
		return "INIT"
	case StateBackup:
		return "BACKUP"
	case StateMaster:
		return "MASTER"
	default:
		return "UNKNOWN"
	}
}

// Stats represents VRRP statistics.
type Stats struct {
	MasterTransitions   uint64
	BackupTransitions   uint64
	AdvertsSent         uint64
	AdvertsReceived     uint64
	InvalidAdverts      uint64
	PriorityZeroReceived uint64
	ChecksumErrors      uint64
}

// NewNode creates a new VRRP node.
func NewNode(config Config) (*Node, error) {
	if err := config.Validate(); err != nil {
		return nil, err
	}

	// Convert interface name
	cInterface := C.CString(config.Interface)
	defer C.free(unsafe.Pointer(cInterface))

	// Convert primary IP
	cPrimaryIP := C.CString(config.PrimaryIP.String())
	defer C.free(unsafe.Pointer(cPrimaryIP))

	// Convert virtual IPs to C array
	// Allocate C array of char pointers
	cVirtualIPsArray := C.malloc(C.size_t(len(config.VirtualIPs)) * C.size_t(unsafe.Sizeof(uintptr(0))))
	defer C.free(cVirtualIPsArray)

	// Convert to slice of pointers for easier manipulation
	cVirtualIPsSlice := (*[1 << 30]*C.char)(cVirtualIPsArray)[:len(config.VirtualIPs):len(config.VirtualIPs)]
	for i, ip := range config.VirtualIPs {
		cVirtualIPsSlice[i] = C.CString(ip.String())
		defer C.free(unsafe.Pointer(cVirtualIPsSlice[i]))
	}

	// Create C config
	cConfig := C.CVrrpConfig{
		vrid:             C.uint8_t(config.VRID),
		priority:         C.uint8_t(config.Priority),
		advert_interval:  C.uint16_t(config.AdvertInterval),
		preempt:          C.bool(config.Preempt),
		_interface:       cInterface,
		primary_ip:       cPrimaryIP,
		virtual_ips:      (**C.char)(cVirtualIPsArray),
		virtual_ip_count: C.size_t(len(config.VirtualIPs)),
	}

	// Create VRRP node
	handle := C.vrrp_new(&cConfig)
	if handle == nil {
		return nil, errors.New("failed to create VRRP node")
	}

	return &Node{handle: handle}, nil
}

// Close frees the VRRP node resources.
func (n *Node) Close() {
	if n.handle != nil {
		C.vrrp_free(n.handle)
		n.handle = nil
	}
}

// Run starts the VRRP state machine (blocks until termination).
func (n *Node) Run() error {
	if n.handle == nil {
		return errors.New("node is closed")
	}

	result := C.vrrp_run(n.handle)
	if result != 0 {
		return errors.New("VRRP run failed")
	}

	return nil
}

// RunAsync starts the VRRP state machine in a background thread.
func (n *Node) RunAsync() error {
	if n.handle == nil {
		return errors.New("node is closed")
	}

	threadHandle := C.vrrp_run_async(n.handle)
	if threadHandle == nil {
		return errors.New("failed to start VRRP async")
	}

	// Note: We don't wait for the thread here - it runs in the background
	return nil
}

// GetState returns the current VRRP state.
func (n *Node) GetState() (State, error) {
	if n.handle == nil {
		return StateInit, errors.New("node is closed")
	}

	cState := C.vrrp_get_state(n.handle)
	return State(cState), nil
}

// GetStats returns VRRP statistics.
func (n *Node) GetStats() (*Stats, error) {
	if n.handle == nil {
		return nil, errors.New("node is closed")
	}

	var cStats C.CVrrpStats
	success := C.vrrp_get_stats(n.handle, &cStats)
	if !success {
		return nil, errors.New("failed to get stats")
	}

	return &Stats{
		MasterTransitions:    uint64(cStats.master_transitions),
		BackupTransitions:    uint64(cStats.backup_transitions),
		AdvertsSent:          uint64(cStats.adverts_sent),
		AdvertsReceived:      uint64(cStats.adverts_received),
		InvalidAdverts:       uint64(cStats.invalid_adverts),
		PriorityZeroReceived: uint64(cStats.priority_zero_received),
		ChecksumErrors:       uint64(cStats.checksum_errors),
	}, nil
}

// Shutdown gracefully shuts down the VRRP node.
func (n *Node) Shutdown() error {
	if n.handle == nil {
		return errors.New("node is closed")
	}

	result := C.vrrp_shutdown(n.handle)
	if result != 0 {
		return errors.New("shutdown failed")
	}

	return nil
}

// Validate validates the VRRP configuration.
func (c *Config) Validate() error {
	if c.VRID == 0 {
		return errors.New("VRID must be between 1 and 255")
	}

	if c.Priority == 0 {
		return errors.New("priority must be between 1 and 255")
	}

	if c.Interface == "" {
		return errors.New("interface name is required")
	}

	if c.PrimaryIP == nil {
		return errors.New("primary IP is required")
	}

	if len(c.VirtualIPs) == 0 {
		return errors.New("at least one virtual IP is required")
	}

	// Check IP version consistency
	isV6 := c.VirtualIPs[0].To4() == nil
	for i, ip := range c.VirtualIPs {
		if (ip.To4() == nil) != isV6 {
			return fmt.Errorf("virtual IP %d has inconsistent IP version", i)
		}
	}

	// Check primary IP matches virtual IP version
	if (c.PrimaryIP.To4() == nil) != isV6 {
		return errors.New("primary IP version must match virtual IP version")
	}

	return nil
}
