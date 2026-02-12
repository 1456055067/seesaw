#!/bin/bash
# Build script for hybrid Rust+Go healthcheck server

set -e

# Source Rust environment if available
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[BUILD]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prereqs() {
    local missing=""

    if ! command -v cargo &> /dev/null; then
        missing="${missing}\n  - cargo (Rust toolchain)"
        log_error "cargo not found. Install from: https://rustup.rs/"
    fi

    if ! command -v go &> /dev/null; then
        missing="${missing}\n  - go (Go toolchain)"
        log_error "go not found. Install from: https://go.dev/dl/"
    fi

    if [ -n "$missing" ]; then
        echo ""
        echo "Missing required tools:$missing"
        echo ""
        echo "To fix:"
        echo "  1. Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        echo "  2. Install Go: https://go.dev/dl/"
        echo "  3. Restart your shell or run: source \$HOME/.cargo/env"
        return 1
    fi

    log_info "Prerequisites OK: cargo $(cargo --version | cut -d' ' -f2), go $(go version | cut -d' ' -f3)"
    return 0
}

# Build Rust healthcheck-server
build_rust() {
    log_step "Building Rust healthcheck server..."
    cd "$PROJECT_ROOT/rust"

    if [ "$1" = "debug" ]; then
        cargo build -p healthcheck-server
        RUST_BIN="$PROJECT_ROOT/rust/target/debug/healthcheck-server"
    else
        cargo build --release -p healthcheck-server
        RUST_BIN="$PROJECT_ROOT/rust/target/release/healthcheck-server"
    fi

    log_info "Rust server built: $RUST_BIN"
}

# Build Go healthcheck-proxy
build_go() {
    log_step "Building Go healthcheck proxy..."
    cd "$PROJECT_ROOT"

    mkdir -p bin

    if [ "$1" = "debug" ]; then
        go build -o bin/healthcheck-proxy ./healthcheck/server/main.go
    else
        go build -ldflags="-s -w" -o bin/healthcheck-proxy ./healthcheck/server/main.go
    fi

    GO_BIN="$PROJECT_ROOT/bin/healthcheck-proxy"
    log_info "Go proxy built: $GO_BIN"
}

# Main
main() {
    log_info "Building Hybrid Healthcheck System"
    log_info "Project root: $PROJECT_ROOT"
    echo ""

    # Check prerequisites
    if ! check_prereqs; then
        exit 1
    fi
    echo ""

    # Parse build type
    BUILD_TYPE="${1:-release}"
    if [ "$BUILD_TYPE" != "debug" ] && [ "$BUILD_TYPE" != "release" ]; then
        echo "Usage: $0 [debug|release]"
        echo "  debug   - Build with debug symbols (faster compilation)"
        echo "  release - Build with optimizations (production)"
        exit 1
    fi

    log_info "Build type: $BUILD_TYPE"
    echo ""

    # Build both components
    build_rust "$BUILD_TYPE"
    build_go "$BUILD_TYPE"

    echo ""
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "Build complete!"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "Binaries:"
    if [ "$BUILD_TYPE" = "debug" ]; then
        echo "  Rust: rust/target/debug/healthcheck-server"
    else
        echo "  Rust: rust/target/release/healthcheck-server"
    fi
    echo "  Go:   bin/healthcheck-proxy"
    echo ""
    echo "Next steps:"
    echo "  1. Start Rust server: ./rust/target/$BUILD_TYPE/healthcheck-server"
    echo "  2. Start Go proxy: ./bin/healthcheck-proxy"
    echo ""
}

main "$@"
