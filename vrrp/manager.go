// Copyright 2024 Google Inc.  All Rights Reserved.
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

// Package vrrp provides a high-performance VRRP implementation for Seesaw HA.
//
// This package wraps the Rust VRRP implementation and provides integration
// with Seesaw's HA system. It can be used as a drop-in replacement for the
// traditional VRRP implementation with significant performance improvements.
//
// Build with: go build -tags rust_vrrp
package vrrp

import (
	"fmt"
	"net"
	"sync"
	"time"

	spb "github.com/google/seesaw/pb/seesaw"
	"github.com/google/seesaw/vrrp/rust"

	log "github.com/golang/glog"
)

// Manager manages a VRRP instance for Seesaw HA.
type Manager struct {
	config Config
	node   *rust.Node

	mu           sync.RWMutex
	state        spb.HaState
	stateChanged chan spb.HaState
	stopChan     chan struct{}
	running      bool
}

// Config specifies the configuration for a VRRP Manager.
type Config struct {
	// VRID is the Virtual Router ID (1-255)
	VRID uint8

	// Priority is the VRRP priority (1-255, 255 = IP owner)
	Priority uint8

	// AdvertInterval is the advertisement interval in centiseconds
	// Default: 100 (1 second)
	AdvertInterval uint16

	// Interface is the network interface name (e.g., "eth0")
	Interface string

	// PrimaryIP is the primary IP address of this node
	PrimaryIP net.IP

	// VirtualIPs are the virtual IP addresses to manage
	VirtualIPs []net.IP

	// Preempt allows higher priority backup to become master
	Preempt bool
}

// NewManager creates a new VRRP Manager.
func NewManager(cfg Config) (*Manager, error) {
	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("invalid config: %v", err)
	}

	// Create Rust VRRP config
	rustCfg := rust.Config{
		VRID:           cfg.VRID,
		Priority:       cfg.Priority,
		AdvertInterval: cfg.AdvertInterval,
		Preempt:        cfg.Preempt,
		Interface:      cfg.Interface,
		PrimaryIP:      cfg.PrimaryIP,
		VirtualIPs:     cfg.VirtualIPs,
	}

	// Create VRRP node
	node, err := rust.NewNode(rustCfg)
	if err != nil {
		return nil, fmt.Errorf("failed to create VRRP node: %v", err)
	}

	m := &Manager{
		config:       cfg,
		node:         node,
		state:        spb.HaState_BACKUP,
		stateChanged: make(chan spb.HaState, 10),
		stopChan:     make(chan struct{}),
	}

	return m, nil
}

// Start starts the VRRP Manager.
func (m *Manager) Start() error {
	m.mu.Lock()
	if m.running {
		m.mu.Unlock()
		return fmt.Errorf("VRRP manager already running")
	}
	m.running = true
	m.mu.Unlock()

	log.Infof("Starting VRRP manager (VRID: %d, Priority: %d, Interface: %s)",
		m.config.VRID, m.config.Priority, m.config.Interface)

	// Start VRRP state machine in background
	go m.runStateMachine()

	// Start state monitor
	go m.monitorState()

	return nil
}

// Stop stops the VRRP Manager gracefully.
func (m *Manager) Stop() error {
	m.mu.Lock()
	if !m.running {
		m.mu.Unlock()
		return nil
	}
	m.running = false
	m.mu.Unlock()

	log.Info("Stopping VRRP manager")

	// Signal stop
	close(m.stopChan)

	// Graceful shutdown
	if err := m.node.Shutdown(); err != nil {
		log.Warningf("VRRP shutdown error: %v", err)
	}

	m.node.Close()

	log.Info("VRRP manager stopped")
	return nil
}

// State returns the current VRRP state.
func (m *Manager) State() spb.HaState {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.state
}

// StateChanged returns a channel that receives state change notifications.
func (m *Manager) StateChanged() <-chan spb.HaState {
	return m.stateChanged
}

// Stats returns VRRP statistics.
func (m *Manager) Stats() (*rust.Stats, error) {
	return m.node.GetStats()
}

// runStateMachine runs the VRRP state machine.
func (m *Manager) runStateMachine() {
	if err := m.node.RunAsync(); err != nil {
		log.Errorf("VRRP state machine error: %v", err)
	}
}

// monitorState monitors VRRP state changes and notifies listeners.
func (m *Manager) monitorState() {
	ticker := time.NewTicker(100 * time.Millisecond)
	defer ticker.Stop()

	for {
		select {
		case <-m.stopChan:
			return
		case <-ticker.C:
			state, err := m.node.GetState()
			if err != nil {
				log.Warningf("Failed to get VRRP state: %v", err)
				continue
			}

			// Map Rust VRRP state to Seesaw HA state
			var haState spb.HaState
			switch state {
			case rust.StateInit:
				haState = spb.HaState_BACKUP
			case rust.StateBackup:
				haState = spb.HaState_BACKUP
			case rust.StateMaster:
				haState = spb.HaState_LEADER
			default:
				log.Warningf("Unknown VRRP state: %v", state)
				haState = spb.HaState_BACKUP
			}

			// Check for state change
			m.mu.Lock()
			if haState != m.state {
				oldState := m.state
				m.state = haState
				m.mu.Unlock()

				log.Infof("VRRP state changed: %s → %s", oldState, haState)

				// Notify listeners (non-blocking)
				select {
				case m.stateChanged <- haState:
				default:
					log.Warning("State change channel full, dropping notification")
				}
			} else {
				m.mu.Unlock()
			}
		}
	}
}

// Validate validates the VRRP configuration.
func (c *Config) Validate() error {
	if c.VRID == 0 {
		return fmt.Errorf("VRID must be between 1 and 255")
	}

	if c.Priority == 0 {
		return fmt.Errorf("priority must be between 1 and 255")
	}

	if c.Interface == "" {
		return fmt.Errorf("interface name is required")
	}

	if c.PrimaryIP == nil {
		return fmt.Errorf("primary IP is required")
	}

	if len(c.VirtualIPs) == 0 {
		return fmt.Errorf("at least one virtual IP is required")
	}

	// Validate IP version consistency
	isV6 := c.VirtualIPs[0].To4() == nil
	for i, ip := range c.VirtualIPs {
		if (ip.To4() == nil) != isV6 {
			return fmt.Errorf("virtual IP %d has inconsistent IP version", i)
		}
	}

	// Validate primary IP matches virtual IP version
	if (c.PrimaryIP.To4() == nil) != isV6 {
		return fmt.Errorf("primary IP version must match virtual IP version")
	}

	// Set default advertisement interval if not specified
	if c.AdvertInterval == 0 {
		c.AdvertInterval = 100 // 1 second
	}

	return nil
}

// DefaultConfig returns a default VRRP configuration.
func DefaultConfig() Config {
	return Config{
		VRID:           1,
		Priority:       100,
		AdvertInterval: 100, // 1 second
		Preempt:        true,
	}
}
