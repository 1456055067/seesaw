//go:build rust_vrrp
// +build rust_vrrp

package vrrp

import (
	"net"
	"testing"
	"time"
)

func TestConfigValidation(t *testing.T) {
	tests := []struct {
		name    string
		config  Config
		wantErr bool
	}{
		{
			name: "valid config",
			config: Config{
				VRID:       1,
				Priority:   100,
				Interface:  "lo",
				PrimaryIP:  net.ParseIP("127.0.0.1"),
				VirtualIPs: []net.IP{net.ParseIP("127.0.1.1")},
			},
			wantErr: false,
		},
		{
			name: "invalid VRID",
			config: Config{
				VRID:       0,
				Priority:   100,
				Interface:  "lo",
				PrimaryIP:  net.ParseIP("127.0.0.1"),
				VirtualIPs: []net.IP{net.ParseIP("127.0.1.1")},
			},
			wantErr: true,
		},
		{
			name: "invalid priority",
			config: Config{
				VRID:       1,
				Priority:   0,
				Interface:  "lo",
				PrimaryIP:  net.ParseIP("127.0.0.1"),
				VirtualIPs: []net.IP{net.ParseIP("127.0.1.1")},
			},
			wantErr: true,
		},
		{
			name: "missing interface",
			config: Config{
				VRID:       1,
				Priority:   100,
				PrimaryIP:  net.ParseIP("127.0.0.1"),
				VirtualIPs: []net.IP{net.ParseIP("127.0.1.1")},
			},
			wantErr: true,
		},
		{
			name: "missing virtual IPs",
			config: Config{
				VRID:      1,
				Priority:  100,
				Interface: "lo",
				PrimaryIP: net.ParseIP("127.0.0.1"),
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.config.Validate()
			if (err != nil) != tt.wantErr {
				t.Errorf("Validate() error = %v, wantErr %v", err, tt.wantErr)
			}
		})
	}
}

func TestDefaultConfig(t *testing.T) {
	cfg := DefaultConfig()

	if cfg.VRID != 1 {
		t.Errorf("DefaultConfig().VRID = %d, want 1", cfg.VRID)
	}

	if cfg.Priority != 100 {
		t.Errorf("DefaultConfig().Priority = %d, want 100", cfg.Priority)
	}

	if cfg.AdvertInterval != 100 {
		t.Errorf("DefaultConfig().AdvertInterval = %d, want 100", cfg.AdvertInterval)
	}

	if !cfg.Preempt {
		t.Error("DefaultConfig().Preempt = false, want true")
	}
}

func TestManagerCreation(t *testing.T) {
	cfg := Config{
		VRID:       1,
		Priority:   100,
		Interface:  "lo",
		PrimaryIP:  net.ParseIP("127.0.0.1"),
		VirtualIPs: []net.IP{net.ParseIP("127.0.2.1")},
	}

	// This will fail without CAP_NET_ADMIN
	mgr, err := NewManager(cfg)
	if err != nil {
		t.Logf("Expected failure without CAP_NET_ADMIN: %v", err)
		return
	}
	defer mgr.Stop()

	// Verify initial state
	state := mgr.State()
	t.Logf("Initial state: %v", state)

	// Try to get stats
	stats, err := mgr.Stats()
	if err != nil {
		t.Errorf("Stats() error = %v", err)
	} else {
		t.Logf("Stats: %+v", stats)
	}
}

func TestManagerStartStop(t *testing.T) {
	cfg := Config{
		VRID:       2,
		Priority:   100,
		Interface:  "lo",
		PrimaryIP:  net.ParseIP("127.0.0.1"),
		VirtualIPs: []net.IP{net.ParseIP("127.0.3.1")},
	}

	mgr, err := NewManager(cfg)
	if err != nil {
		t.Logf("Skipping test (requires CAP_NET_ADMIN): %v", err)
		return
	}
	defer mgr.Stop()

	// Start manager
	if err := mgr.Start(); err != nil {
		t.Fatalf("Start() error = %v", err)
	}

	// Wait a bit
	time.Sleep(500 * time.Millisecond)

	// Stop manager
	if err := mgr.Stop(); err != nil {
		t.Errorf("Stop() error = %v", err)
	}

	// Double stop should not error
	if err := mgr.Stop(); err != nil {
		t.Errorf("Second Stop() error = %v", err)
	}

	// Start after stop should fail
	if err := mgr.Start(); err == nil {
		t.Error("Start() after Stop() should fail")
	}
}

func TestStateMonitoring(t *testing.T) {
	cfg := Config{
		VRID:       3,
		Priority:   255, // IP owner, becomes master immediately
		Interface:  "lo",
		PrimaryIP:  net.ParseIP("127.0.0.1"),
		VirtualIPs: []net.IP{net.ParseIP("127.0.4.1")},
	}

	mgr, err := NewManager(cfg)
	if err != nil {
		t.Logf("Skipping test (requires CAP_NET_ADMIN): %v", err)
		return
	}
	defer mgr.Stop()

	if err := mgr.Start(); err != nil {
		t.Fatalf("Start() error = %v", err)
	}

	// Wait for state change notification
	select {
	case state := <-mgr.StateChanged():
		t.Logf("Received state change: %v", state)
	case <-time.After(2 * time.Second):
		t.Log("No state change received (timeout)")
	}

	// Check final state
	finalState := mgr.State()
	t.Logf("Final state: %v", finalState)

	// Get stats
	stats, err := mgr.Stats()
	if err != nil {
		t.Errorf("Stats() error = %v", err)
	} else {
		t.Logf("Final stats: %+v", stats)
	}
}
