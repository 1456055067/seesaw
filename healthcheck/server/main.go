// Copyright 2024 Google Inc. All Rights Reserved.
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

// Package main implements a thin RPC proxy between Seesaw Engine and Rust healthcheck server.
package main

import (
	"bufio"
	"encoding/json"
	"flag"
	"fmt"
	"net"
	"net/rpc"
	"os"
	"time"

	"github.com/google/seesaw/common/ipc"
	"github.com/google/seesaw/common/seesaw"
	"github.com/google/seesaw/healthcheck"

	log "github.com/golang/glog"
)

const (
	engineTimeout  = 10 * time.Second
	fetchInterval  = 15 * time.Second
	rustSocketPath = "/var/run/seesaw/healthcheck-proxy.sock"
)

var (
	engineSocket = flag.String("engine_socket", seesaw.EngineSocket, "Seesaw Engine socket")
	rustSocket   = flag.String("rust_socket", rustSocketPath, "Rust server socket")
)

// ProxyToServerMsg represents messages sent from this proxy to the Rust server
type ProxyToServerMsg struct {
	Type    string                          `json:"type"`
	Configs []map[string]interface{}        `json:"configs,omitempty"`
}

// ServerToProxyMsg represents messages sent from the Rust server to this proxy
type ServerToProxyMsg struct {
	Type         string                   `json:"type"`
	Batch        *NotificationBatch       `json:"batch,omitempty"`
	Statuses     []map[string]interface{} `json:"statuses,omitempty"`
	Message      string                   `json:"message,omitempty"`
}

// NotificationBatch contains a batch of healthcheck status notifications
type NotificationBatch struct {
	Notifications []Notification `json:"notifications"`
}

// Notification represents a single healthcheck status notification
type Notification struct {
	ID     uint64 `json:"id"`
	Status Status `json:"status"`
}

// Status represents healthcheck status
type Status struct {
	LastCheck *time.Time    `json:"last_check,omitempty"`
	Duration  time.Duration `json:"duration"`
	Failures  uint64        `json:"failures"`
	Successes uint64        `json:"successes"`
	State     string        `json:"state"`
	Message   string        `json:"message"`
}

func main() {
	flag.Parse()

	log.Info("Seesaw Healthcheck RPC Proxy starting")

	// Connect to Rust server
	conn, err := net.Dial("unix", *rustSocket)
	if err != nil {
		log.Fatalf("Failed to connect to Rust server at %s: %v", *rustSocket, err)
	}
	defer conn.Close()

	log.Infof("Connected to Rust server at %s", *rustSocket)

	// Start config fetcher
	go configFetcher(conn)

	// Handle notifications from Rust server
	notificationHandler(conn)
}

// configFetcher periodically fetches configs from Engine and sends to Rust
func configFetcher(conn net.Conn) {
	writer := bufio.NewWriter(conn)

	for {
		configs, err := getHealthchecks()
		if err != nil {
			log.Errorf("Failed to get healthchecks: %v", err)
			time.Sleep(5 * time.Second)
			continue
		}

		// Convert to JSON-serializable format
		configList := make([]map[string]interface{}, 0, len(configs.Configs))
		for id, cfg := range configs.Configs {
			configMap := map[string]interface{}{
				"id":       uint64(id),
				"interval": cfg.Interval.String(),
				"timeout":  cfg.Timeout.String(),
				// Add checker details - simplified for now
				"checker": map[string]interface{}{
					"type": "tcp", // Will be enhanced based on actual checker type
				},
			}
			configList = append(configList, configMap)
		}

		msg := ProxyToServerMsg{
			Type:    "update_configs",
			Configs: configList,
		}

		data, err := json.Marshal(msg)
		if err != nil {
			log.Errorf("Failed to marshal configs: %v", err)
			continue
		}

		_, err = writer.Write(append(data, '\n'))
		if err != nil {
			log.Errorf("Failed to write configs: %v", err)
			continue
		}

		if err := writer.Flush(); err != nil {
			log.Errorf("Failed to flush: %v", err)
			continue
		}

		log.Infof("Sent %d healthcheck configs to Rust server", len(configList))
		time.Sleep(fetchInterval)
	}
}

// notificationHandler reads notifications from Rust and sends to Engine
func notificationHandler(conn net.Conn) {
	scanner := bufio.NewScanner(conn)

	for scanner.Scan() {
		var msg ServerToProxyMsg
		if err := json.Unmarshal(scanner.Bytes(), &msg); err != nil {
			log.Errorf("Failed to parse message from Rust: %v", err)
			continue
		}

		switch msg.Type {
		case "notification_batch":
			if msg.Batch != nil {
				if err := sendBatch(msg.Batch.Notifications); err != nil {
					log.Errorf("Failed to send batch: %v", err)
				}
			}
		case "ready":
			log.Info("Rust server ready")
		case "error":
			log.Errorf("Rust server error: %s", msg.Message)
		}
	}

	if err := scanner.Err(); err != nil {
		log.Fatalf("Scanner error: %v", err)
	}
}

// getHealthchecks fetches current healthcheck configs from Engine
func getHealthchecks() (*healthcheck.Checks, error) {
	engineConn, err := net.DialTimeout("unix", *engineSocket, engineTimeout)
	if err != nil {
		return nil, fmt.Errorf("dial failed: %v", err)
	}
	defer engineConn.Close()

	engineConn.SetDeadline(time.Now().Add(engineTimeout))
	engine := rpc.NewClient(engineConn)
	defer engine.Close()

	var checks healthcheck.Checks
	ctx := ipc.NewTrustedContext(seesaw.SCHealthcheck)
	if err := engine.Call("SeesawEngine.Healthchecks", ctx, &checks); err != nil {
		return nil, fmt.Errorf("SeesawEngine.Healthchecks failed: %v", err)
	}

	return &checks, nil
}

// sendBatch sends a batch of notifications to Engine
func sendBatch(notifications []Notification) error {
	if len(notifications) == 0 {
		return nil
	}

	// Convert to healthcheck.Notification format
	batch := make([]*healthcheck.Notification, 0, len(notifications))
	for _, n := range notifications {
		state := healthcheck.StateUnknown
		switch n.Status.State {
		case "healthy":
			state = healthcheck.StateHealthy
		case "unhealthy":
			state = healthcheck.StateUnhealthy
		}

		lastCheck := time.Now()
		if n.Status.LastCheck != nil {
			lastCheck = *n.Status.LastCheck
		}

		batch = append(batch, &healthcheck.Notification{
			Id: healthcheck.Id(n.ID),
			Status: healthcheck.Status{
				LastCheck: lastCheck,
				Duration:  n.Status.Duration,
				Failures:  n.Status.Failures,
				Successes: n.Status.Successes,
				State:     state,
				Message:   n.Status.Message,
			},
		})
	}

	engineConn, err := net.DialTimeout("unix", *engineSocket, engineTimeout)
	if err != nil {
		return err
	}
	defer engineConn.Close()

	engineConn.SetDeadline(time.Now().Add(engineTimeout))
	engine := rpc.NewClient(engineConn)
	defer engine.Close()

	var reply int
	ctx := ipc.NewTrustedContext(seesaw.SCHealthcheck)
	return engine.Call("SeesawEngine.HealthState", &healthcheck.HealthState{ctx, batch}, &reply)
}
