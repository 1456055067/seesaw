// Copyright 2012 Google Inc. All Rights Reserved.
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

// Author: jsing@google.com (Joel Sing)

package engine

import (
	"testing"

	"github.com/google/seesaw/common/seesaw"
	"github.com/google/seesaw/healthcheck"
)

func TestEnableDisableBackend(t *testing.T) {
	v := newTestVserver(nil)
	v.handleConfigUpdate(&vserverConfig)

	// Bring everything up.
	for _, c := range v.checks {
		n := &checkNotification{key: c.key, status: healthcheck.Status{State: healthcheck.StateHealthy}}
		v.handleCheckNotification(n)
	}

	// Verify all destinations are healthy and active.
	for _, svc := range v.services {
		for _, d := range svc.dests {
			if !d.healthy {
				t.Errorf("destination %v: expected healthy, got unhealthy", d.destinationKey)
			}
		}
	}

	// Disable backend1 via BackendOverride.
	o := &seesaw.BackendOverride{
		Hostname:      backend1.Hostname,
		OverrideState: seesaw.OverrideDisable,
	}
	v.handleOverride(o)

	// Verify backend1 destinations are unhealthy, backend2 still healthy.
	for _, svc := range v.services {
		for _, d := range svc.dests {
			if d.backend.Hostname == backend1.Hostname {
				if d.healthy {
					t.Errorf("destination %v (backend1): expected unhealthy after disable, got healthy", d.destinationKey)
				}
			} else {
				if !d.healthy {
					t.Errorf("destination %v (backend2): expected healthy, got unhealthy", d.destinationKey)
				}
			}
		}
	}

	// Re-enable backend1 via OverrideDefault.
	o = &seesaw.BackendOverride{
		Hostname:      backend1.Hostname,
		OverrideState: seesaw.OverrideDefault,
	}
	v.handleOverride(o)

	// Force a state transition by cycling unhealthy→healthy, since
	// handleCheckNotification only acts on transitions.
	for _, c := range v.checks {
		n := &checkNotification{key: c.key, status: healthcheck.Status{State: healthcheck.StateUnhealthy}}
		v.handleCheckNotification(n)
	}
	for _, c := range v.checks {
		n := &checkNotification{key: c.key, status: healthcheck.Status{State: healthcheck.StateHealthy}}
		v.handleCheckNotification(n)
	}

	// Verify all destinations are healthy again.
	for _, svc := range v.services {
		for _, d := range svc.dests {
			if !d.healthy {
				t.Errorf("destination %v: expected healthy after re-enable, got unhealthy", d.destinationKey)
			}
		}
	}
}
