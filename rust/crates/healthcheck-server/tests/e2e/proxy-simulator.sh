#!/usr/bin/env bash
# E2E Proxy Simulator
#
# Simulates the Go proxy by connecting to the Rust healthcheck-server
# via Unix socket. Sends healthcheck configs and streams all received
# messages to stdout for the test runner to validate.

set -euo pipefail

SOCKET_PATH="/var/run/seesaw/healthcheck.sock"

log() { echo "[proxy-sim] $1"; }

# Resolve target-service Docker DNS name to IP address.
# The real Go proxy sends resolved IPs, not hostnames.
TARGET_IP=$(getent hosts target-service | awk '{print $1}' | head -1)
if [ -z "$TARGET_IP" ]; then
    log "ERROR: Cannot resolve target-service hostname"
    exit 1
fi
log "Target IP: $TARGET_IP"

# Wait for Unix socket to appear
log "Waiting for socket at $SOCKET_PATH..."
for i in $(seq 1 60); do
    if [ -S "$SOCKET_PATH" ]; then
        break
    fi
    sleep 0.5
done

if [ ! -S "$SOCKET_PATH" ]; then
    log "ERROR: Socket not found after 30s"
    exit 1
fi
log "Socket found"

# Connect via socat using bash coproc for bidirectional communication
coproc SOCK { socat - UNIX-CONNECT:"$SOCKET_PATH"; }
sleep 0.5

# Read the Ready message (first line from server)
READY=""
read -t 10 READY <&${SOCK[0]} || true
if [ -n "$READY" ]; then
    log "RECV: $READY"
    MSG_TYPE=$(echo "$READY" | jq -r '.type' 2>/dev/null || echo "")
    if [ "$MSG_TYPE" = "ready" ]; then
        log "Ready message OK"
    else
        log "ERROR: Expected type=ready, got: $READY"
        exit 1
    fi
else
    log "ERROR: No ready message received within 10s"
    exit 1
fi

# Send healthcheck configs: TCP (id=1) and HTTP (id=2) targeting the nginx service
CONFIG_MSG='{"type":"update_configs","configs":[{"id":1,"interval":"2s","timeout":"1s","retries":2,"checker_type":"tcp","ip":"'"$TARGET_IP"'","port":80},{"id":2,"interval":"2s","timeout":"1s","retries":2,"checker_type":"http","ip":"'"$TARGET_IP"'","port":80,"method":"GET","path":"/health","expected_codes":[200],"secure":false}]}'

echo "$CONFIG_MSG" >&${SOCK[1]}
log "SENT: update_configs (TCP id=1, HTTP id=2)"
log "CONFIG_SENT"

# Continuously read and log all messages from the server.
# The test runner (run-e2e-tests.sh) monitors these logs for assertions.
while true; do
    LINE=""
    read -t 3 LINE <&${SOCK[0]} || true
    if [ -n "$LINE" ]; then
        log "RECV: $LINE"
    fi
done
