// Copyright 2013 Google Inc. All Rights Reserved.
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
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/pem"
	"errors"
	"fmt"
	"math/big"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	spb "github.com/google/seesaw/pb/seesaw"
)

func newLocalTCPListener() (*net.TCPListener, *net.TCPAddr, error) {
	tcpAddr, err := net.ResolveTCPAddr("tcp", "localhost:0")
	if err != nil {
		return nil, nil, err
	}
	l, err := net.ListenTCP("tcp", tcpAddr)
	if err != nil {
		return nil, nil, err
	}
	tcpAddr = l.Addr().(*net.TCPAddr)

	return l, tcpAddr, nil
}

type testNoteDispatcher struct {
	notes chan *SyncNote
}

func newTestNoteDispatcher() *testNoteDispatcher {
	return &testNoteDispatcher{
		notes: make(chan *SyncNote, sessionNotesQueueSize),
	}
}

func (tsd *testNoteDispatcher) dispatch(note *SyncNote) {
	tsd.notes <- note
}

func (tsd *testNoteDispatcher) nextNote() (*SyncNote, error) {
	select {
	case n := <-tsd.notes:
		return n, nil
	case <-time.After(time.Second):
		return nil, errors.New("timed out waiting for note")
	}
}

// generateTestCerts creates a self-signed CA and node certificate in a temp
// directory and returns the directory path. The caller should defer os.RemoveAll
// on the returned path.
func generateTestCerts(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()

	// Generate CA key and self-signed cert.
	caKey, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatalf("Failed to generate CA key: %v", err)
	}
	caTemplate := &x509.Certificate{
		SerialNumber:          big.NewInt(1),
		Subject:               pkix.Name{CommonName: "Test CA"},
		NotBefore:             time.Now().Add(-time.Hour),
		NotAfter:              time.Now().Add(time.Hour),
		IsCA:                  true,
		BasicConstraintsValid: true,
		KeyUsage:              x509.KeyUsageCertSign | x509.KeyUsageCRLSign,
	}
	caCertDER, err := x509.CreateCertificate(rand.Reader, caTemplate, caTemplate, &caKey.PublicKey, caKey)
	if err != nil {
		t.Fatalf("Failed to create CA certificate: %v", err)
	}
	writePEM(t, filepath.Join(dir, "ca.crt"), "CERTIFICATE", caCertDER)

	// Generate node key and cert signed by the CA.
	nodeKey, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatalf("Failed to generate node key: %v", err)
	}
	nodeTemplate := &x509.Certificate{
		SerialNumber: big.NewInt(2),
		Subject:      pkix.Name{CommonName: "localhost"},
		NotBefore:    time.Now().Add(-time.Hour),
		NotAfter:     time.Now().Add(time.Hour),
		KeyUsage:     x509.KeyUsageDigitalSignature,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth, x509.ExtKeyUsageClientAuth},
		IPAddresses:  []net.IP{net.IPv4(127, 0, 0, 1), net.IPv6loopback},
	}
	caCert, err := x509.ParseCertificate(caCertDER)
	if err != nil {
		t.Fatalf("Failed to parse CA certificate: %v", err)
	}
	nodeCertDER, err := x509.CreateCertificate(rand.Reader, nodeTemplate, caCert, &nodeKey.PublicKey, caKey)
	if err != nil {
		t.Fatalf("Failed to create node certificate: %v", err)
	}
	writePEM(t, filepath.Join(dir, "seesaw.crt"), "CERTIFICATE", nodeCertDER)

	nodeKeyDER, err := x509.MarshalECPrivateKey(nodeKey)
	if err != nil {
		t.Fatalf("Failed to marshal node key: %v", err)
	}
	writePEM(t, filepath.Join(dir, "seesaw.key"), "EC PRIVATE KEY", nodeKeyDER)

	return dir
}

func writePEM(t *testing.T, path, typ string, data []byte) {
	t.Helper()
	f, err := os.Create(path)
	if err != nil {
		t.Fatalf("Failed to create %s: %v", path, err)
	}
	defer f.Close()
	if err := pem.Encode(f, &pem.Block{Type: typ, Bytes: data}); err != nil {
		t.Fatalf("Failed to write PEM to %s: %v", path, err)
	}
}

func newSyncTest(t *testing.T) (net.Listener, *syncClient, *syncServer, *testNoteDispatcher, error) {
	t.Helper()
	ln, addr, err := newLocalTCPListener()
	if err != nil {
		return nil, nil, nil, nil, fmt.Errorf("Failed to create local TCP listener: %v", err)
	}

	certDir := generateTestCerts(t)

	engine := newTestEngine()
	engine.haManager.statusLock.Lock()
	engine.haManager.status.State = spb.HaState_LEADER
	engine.haManager.statusLock.Unlock()
	engine.config.Node.IPv4Addr = addr.IP
	engine.config.Peer.IPv4Addr = addr.IP
	engine.config.SyncPort = addr.Port
	engine.config.CACertFile = filepath.Join(certDir, "ca.crt")
	engine.config.CertFile = filepath.Join(certDir, "seesaw.crt")
	engine.config.KeyFile = filepath.Join(certDir, "seesaw.key")

	tlsConfig, err := engine.syncTLSConfig()
	if err != nil {
		ln.Close()
		return nil, nil, nil, nil, fmt.Errorf("Failed to create TLS config: %v", err)
	}
	tlsListener := tls.NewListener(ln, tlsConfig)

	server := newSyncServer(engine)
	go server.serve(tlsListener)

	client := newSyncClient(engine)
	dispatcher := newTestNoteDispatcher()
	client.dispatch = dispatcher.dispatch

	return tlsListener, client, server, dispatcher, nil
}

func TestBasicSync(t *testing.T) {
	ln, client, server, dispatcher, err := newSyncTest(t)
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()

	go client.runOnce()
	defer func() { client.quit <- true }()

	// The server should send a desync at the start of the session.
	n, err := dispatcher.nextNote()
	if err != nil {
		t.Fatalf("Expected initial desync, got error: %v", err)
	}
	if n.Type != SNTDesync {
		t.Fatalf("Initial note type = %v, want %v", n.Type, SNTDesync)
	}

	// Send a notification for each sync note type.
	for nt := range syncNoteTypeNames {
		server.notify(&SyncNote{Type: nt})
		n, err := dispatcher.nextNote()
		if err != nil {
			t.Fatalf("After sending %v, nextNote failed: %v", nt, err)
		}
		if n.Type != nt {
			t.Errorf("After sending %v, nextNote = %v, want %v", nt, n, nt)
		}
	}
}

func TestSyncHeartbeats(t *testing.T) {
	ln, client, server, dispatcher, err := newSyncTest(t)
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()

	server.heartbeatInterval = 500 * time.Millisecond
	go server.run()
	go client.run()

	// Enabling briefly shouldn't have time to get a heartbeat, but should get
	// the initial desync.
	client.enable()
	// Make sure we can read the desync (and flush it so the later client doesn't
	// see it).
	if n, err := dispatcher.nextNote(); err != nil || n.Type != SNTDesync {
		t.Errorf("During short enablement, nextNote() = %v, %v; expected desync", n, err)
	} else {
		t.Logf("Got short note: %v, %v", n, err)
	}
	client.disable()

	// Now let it run long enough to get some heartbeats.
	// Any heartbeats sent to the first enablement of the client shouldn't appear here.
	client.enable()
	time.Sleep(2*server.heartbeatInterval + 50*time.Millisecond)
	client.disable()
	// Waiting longer after disabling shouldn't send another heartbeat (and
	// should make sure we receive the ones we had queued).
	time.Sleep(server.heartbeatInterval + 50*time.Millisecond)

	wantNotes := []SyncNoteType{SNTDesync, SNTHeartbeat, SNTHeartbeat}
	for _, nt := range wantNotes {
		n, err := dispatcher.nextNote()
		if err != nil || n.Type != nt {
			t.Errorf("After long enablement, nextNote() = %v, %v; want Type = %v", n, err, nt)
			continue
		}
		t.Logf("After long enablement: got sync note %v", n)
	}

	n, err := dispatcher.nextNote()
	if err != nil {
		if !strings.Contains(err.Error(), "timed out") {
			t.Errorf("Expected timeout error, got: %v", err)
		}
		return
	}
	// If we end up registering near the heartbeat boundaries, we might
	// legitimately get 3 heartbeats, but no more!
	if n.Type != SNTHeartbeat {
		t.Errorf("After long enablement, got additional note %v, expected only %d notes", n, len(wantNotes))
	} else {
		n, err := dispatcher.nextNote()
		if err == nil {
			t.Errorf("After long enablement, got additional note %v, expected at most %d notes", n, len(wantNotes)+1)
		}
	}
}

func TestSyncDesync(t *testing.T) {
	ln, client, server, dispatcher, err := newSyncTest(t)
	if err != nil {
		t.Fatal(err)
	}
	defer ln.Close()

	// Switch the dispatcher to a blocking channel.
	dispatcher.notes = make(chan *SyncNote)

	go client.runOnce()
	defer func() {
		close(client.quit)
		for {
			// Drain the notes to unblock the quit reader.
			if _, err := dispatcher.nextNote(); err != nil {
				break
			}
		}
		<-client.stopped
	}()

	// The server should send a desync at the start of the session.
	n, err := dispatcher.nextNote()
	if err != nil {
		t.Fatalf("Expected desync, got error: %v", err)
	}
	if n.Type != SNTDesync {
		t.Fatalf("Got %v, want %v", n.Type, SNTDesync)
	}

	// Send a single notification to complete the poll.
	server.notify(&SyncNote{Type: SNTHeartbeat})
	time.Sleep(250 * time.Millisecond)
	// syncClient.poll should now be blocked writing that Note to
	// testNoteDispatcher's notes chan.

	// Send enough notifications to fill the session channel buffer.
	server.notify(&SyncNote{Type: SNTHeartbeat})
	server.notify(&SyncNote{Type: SNTConfigUpdate})
	for i := 0; i < (sessionNotesQueueSize * 1.1); i++ {
		server.notify(&SyncNote{Type: SNTHealthcheck})
	}

	// Now we unblock syncClient.poll by reading the initial notification from
	// the local chan. At that point, it should do another Poll and see the
	// desync.
	received := make(map[SyncNoteType]int)
noteLoop:
	for i := 0; i < (sessionNotesQueueSize * 1.1); i++ {
		n, err := dispatcher.nextNote()
		if err != nil {
			t.Fatalf("nextNote() failed: %v; expected note", err)
		}
		received[n.Type]++

		switch n.Type {
		case SNTHeartbeat, SNTHealthcheck: // ok
		case SNTDesync:
			break noteLoop
		default:
			t.Fatalf("Unexpected notification %v", n)
		}
	}
	if received[SNTDesync] != 1 {
		t.Errorf("While waiting for desync, received: %v; expected 1 Desync", received)
	}
	if received[SNTHeartbeat] != 1 {
		t.Errorf("While waiting for desync, received: %v; expected 1 Heartbeat", received)
	}
}
