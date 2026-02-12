#!/bin/bash
# Comprehensive check script for healthcheck-server
# Runs: cargo fmt, clippy, audit, test, and build

set -e

# Source Rust environment
if [ -f "$HOME/.cargo/env" ]; then
    source "$HOME/.cargo/env"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RUST_DIR="$PROJECT_ROOT/rust"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
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
    log_step "Checking prerequisites..."

    if ! command -v cargo &> /dev/null; then
        log_error "cargo not found"
        echo "Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        return 1
    fi

    log_info "cargo $(cargo --version)"

    # Check for clippy and rustfmt
    if ! cargo clippy --version &> /dev/null; then
        log_warn "clippy not installed, installing..."
        rustup component add clippy
    fi

    if ! cargo fmt --version &> /dev/null; then
        log_warn "rustfmt not installed, installing..."
        rustup component add rustfmt
    fi

    return 0
}

# Run cargo fmt
run_fmt() {
    log_step "Running cargo fmt..."
    cd "$RUST_DIR"

    # Check formatting first
    if ! cargo fmt --check -p healthcheck-server 2>&1; then
        log_warn "Code is not formatted, auto-formatting..."
        cargo fmt -p healthcheck-server
        log_info "Code formatted"
    else
        log_info "Code is properly formatted"
    fi
}

# Run cargo clippy
run_clippy() {
    log_step "Running cargo clippy..."
    cd "$RUST_DIR"

    if cargo clippy -p healthcheck-server --all-targets --all-features -- -D warnings 2>&1; then
        log_info "No clippy warnings"
    else
        log_error "Clippy found issues"
        return 1
    fi
}

# Run cargo audit
run_audit() {
    log_step "Running cargo audit..."
    cd "$RUST_DIR"

    # Install cargo-audit if not present
    if ! cargo audit --version &> /dev/null; then
        log_warn "cargo-audit not installed, installing..."
        cargo install cargo-audit
    fi

    if cargo audit 2>&1; then
        log_info "No security vulnerabilities found"
    else
        log_warn "Security audit found issues (review above)"
    fi
}

# Run cargo test
run_tests() {
    log_step "Running cargo test..."
    cd "$RUST_DIR"

    if cargo test -p healthcheck-server 2>&1; then
        log_info "All tests passed"
    else
        log_error "Tests failed"
        return 1
    fi
}

# Run cargo build
run_build() {
    log_step "Running cargo build..."
    cd "$RUST_DIR"

    local build_type="${1:-debug}"

    if [ "$build_type" = "release" ]; then
        if cargo build --release -p healthcheck-server 2>&1; then
            log_info "Release build successful: target/release/healthcheck-server"
        else
            log_error "Release build failed"
            return 1
        fi
    else
        if cargo build -p healthcheck-server 2>&1; then
            log_info "Debug build successful: target/debug/healthcheck-server"
        else
            log_error "Debug build failed"
            return 1
        fi
    fi
}

# Main
main() {
    log_info "Healthcheck Server Comprehensive Check"
    log_info "Project: $PROJECT_ROOT"
    echo ""

    # Parse options
    SKIP_TESTS=false
    SKIP_AUDIT=false
    BUILD_TYPE="debug"

    while [[ $# -gt 0 ]]; do
        case $1 in
            --skip-tests)
                SKIP_TESTS=true
                shift
                ;;
            --skip-audit)
                SKIP_AUDIT=true
                shift
                ;;
            --release)
                BUILD_TYPE="release"
                shift
                ;;
            *)
                echo "Usage: $0 [--skip-tests] [--skip-audit] [--release]"
                exit 1
                ;;
        esac
    done

    # Run checks
    if ! check_prereqs; then
        exit 1
    fi
    echo ""

    run_fmt
    echo ""

    run_clippy || {
        log_error "Fix clippy warnings before proceeding"
        exit 1
    }
    echo ""

    if [ "$SKIP_AUDIT" = false ]; then
        run_audit
        echo ""
    fi

    if [ "$SKIP_TESTS" = false ]; then
        run_tests || {
            log_error "Fix test failures before proceeding"
            exit 1
        }
        echo ""
    fi

    run_build "$BUILD_TYPE" || {
        log_error "Build failed"
        exit 1
    }
    echo ""

    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    log_info "✓ All checks passed!"
    log_info "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    echo "Next steps:"
    echo "  1. Run integration tests: ./rust/crates/healthcheck-server/tests/integration_test.sh"
    echo "  2. Build Go proxy: go build -o bin/healthcheck-proxy ./healthcheck/server/main.go"
    echo "  3. Test hybrid system: ./rust/crates/healthcheck-server/tests/hybrid_integration_test.sh"
}

main "$@"
