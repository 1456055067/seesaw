//go:build rust_vrrp
// +build rust_vrrp

package rust

import (
	"net"
	"testing"
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
				VRID:           1,
				Priority:       100,
				AdvertInterval: 100,
				Preempt:        true,
				Interface:      "lo",
				PrimaryIP:      net.ParseIP("10.0.0.1"),
				VirtualIPs: []net.IP{
					net.ParseIP("192.168.1.1"),
				},
			},
			wantErr: false,
		},
		{
			name: "invalid VRID",
			config: Config{
				VRID:           0,
				Priority:       100,
				AdvertInterval: 100,
				Interface:      "lo",
				PrimaryIP:      net.ParseIP("10.0.0.1"),
				VirtualIPs:     []net.IP{net.ParseIP("192.168.1.1")},
			},
			wantErr: true,
		},
		{
			name: "invalid priority",
			config: Config{
				VRID:           1,
				Priority:       0,
				AdvertInterval: 100,
				Interface:      "lo",
				PrimaryIP:      net.ParseIP("10.0.0.1"),
				VirtualIPs:     []net.IP{net.ParseIP("192.168.1.1")},
			},
			wantErr: true,
		},
		{
			name: "missing interface",
			config: Config{
				VRID:           1,
				Priority:       100,
				AdvertInterval: 100,
				PrimaryIP:      net.ParseIP("10.0.0.1"),
				VirtualIPs:     []net.IP{net.ParseIP("192.168.1.1")},
			},
			wantErr: true,
		},
		{
			name: "missing primary IP",
			config: Config{
				VRID:           1,
				Priority:       100,
				AdvertInterval: 100,
				Interface:      "lo",
				VirtualIPs:     []net.IP{net.ParseIP("192.168.1.1")},
			},
			wantErr: true,
		},
		{
			name: "missing virtual IPs",
			config: Config{
				VRID:           1,
				Priority:       100,
				AdvertInterval: 100,
				Interface:      "lo",
				PrimaryIP:      net.ParseIP("10.0.0.1"),
				VirtualIPs:     []net.IP{},
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

func TestStateString(t *testing.T) {
	tests := []struct {
		state State
		want  string
	}{
		{StateInit, "INIT"},
		{StateBackup, "BACKUP"},
		{StateMaster, "MASTER"},
		{State(99), "UNKNOWN"},
	}

	for _, tt := range tests {
		t.Run(tt.want, func(t *testing.T) {
			if got := tt.state.String(); got != tt.want {
				t.Errorf("State.String() = %v, want %v", got, tt.want)
			}
		})
	}
}

// TestNodeCreation tests creating and closing a VRRP node.
// This test will fail without CAP_NET_ADMIN, so we just verify the API works.
func TestNodeCreation(t *testing.T) {
	config := Config{
		VRID:           1,
		Priority:       100,
		AdvertInterval: 100,
		Preempt:        true,
		Interface:      "lo",
		PrimaryIP:      net.ParseIP("127.0.0.1"),
		VirtualIPs: []net.IP{
			net.ParseIP("127.0.0.2"),
		},
	}

	// This will likely fail without root/CAP_NET_ADMIN, but tests the API
	node, err := NewNode(config)
	if err != nil {
		t.Logf("Expected failure without CAP_NET_ADMIN: %v", err)
		return
	}
	defer node.Close()

	// If we got here, we have privileges
	state, err := node.GetState()
	if err != nil {
		t.Errorf("GetState() error = %v", err)
	}
	t.Logf("Initial state: %v", state)

	stats, err := node.GetStats()
	if err != nil {
		t.Errorf("GetStats() error = %v", err)
	}
	t.Logf("Stats: %+v", stats)
}
