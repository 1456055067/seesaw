#!/bin/bash
# Script to fix common clippy warnings

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

main() {
    log_info "Running clippy with auto-fix suggestions"
    cd "$RUST_DIR"

    # Check if clippy is installed
    if ! cargo clippy --version &> /dev/null; then
        log_warn "clippy not installed, installing..."
        rustup component add clippy
    fi

    log_step "Running clippy fix (automatically fixable issues)..."

    # Run clippy with --fix flag to auto-fix issues
    if cargo clippy --fix -p healthcheck-server --allow-dirty --allow-staged 2>&1; then
        log_info "Auto-fix complete"
    else
        log_warn "Some issues require manual fixing"
    fi

    echo ""
    log_step "Running clippy check for remaining issues..."

    # Run clippy check to see what's left
    if cargo clippy -p healthcheck-server --all-targets --all-features -- -D warnings 2>&1; then
        log_info "✓ No clippy warnings!"
    else
        log_warn "Manual fixes needed (see above)"
        echo ""
        echo "Common fixes:"
        echo "  - Remove unused imports"
        echo "  - Simplify boolean expressions"
        echo "  - Use .as_deref() instead of .clone().unwrap_or()"
        echo "  - Replace .to_string() with .into() for literals"
        echo "  - Remove redundant field names in struct init"
        return 1
    fi

    log_info "All clippy checks passed!"
}

main "$@"
