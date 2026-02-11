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

// This file contains types and functions that are used across multiple
// engine tests.

import (
	"net"

	"github.com/google/seesaw/common/seesaw"
	"github.com/google/seesaw/engine/config"
	ncclient "github.com/google/seesaw/ncc/client"
)

func newTestEngine() *Engine {
	cfg := config.DefaultEngineConfig()
	cfg.Node = seesaw.Host{
		Hostname: "seesaw1.example.com",
		IPv4Addr: net.ParseIP("10.0.0.1"),
		IPv4Mask: net.CIDRMask(24, 32),
	}
	cfg.Peer = seesaw.Host{
		Hostname: "seesaw2.example.com",
		IPv4Addr: net.ParseIP("10.0.0.2"),
		IPv4Mask: net.CIDRMask(24, 32),
	}
	cfg.ClusterVIP = seesaw.Host{
		Hostname: "seesaw-vip.example.com",
		IPv4Addr: net.ParseIP("10.0.0.100"),
		IPv4Mask: net.CIDRMask(24, 32),
	}
	e := newEngineWithNCC(&cfg, ncclient.NewDummyNCC())
	e.lbInterface = ncclient.NewDummyLBInterface()
	return e
}

func newTestVserver(engine *Engine) *vserver {
	if engine == nil {
		engine = newTestEngine()
	}
	v := newVserver(engine)
	return v
}
