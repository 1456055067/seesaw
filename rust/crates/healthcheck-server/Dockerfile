# Multi-stage build for healthcheck-server
# Build context must be the rust/ workspace root

# Stage 1: Build
FROM rust:1.93-trixie AS builder

WORKDIR /build

# Copy workspace manifests and lock file first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/common/Cargo.toml crates/common/Cargo.toml
COPY crates/healthcheck/Cargo.toml crates/healthcheck/Cargo.toml
COPY crates/healthcheck-ffi/Cargo.toml crates/healthcheck-ffi/Cargo.toml
COPY crates/healthcheck-server/Cargo.toml crates/healthcheck-server/Cargo.toml
COPY crates/ipvs/Cargo.toml crates/ipvs/Cargo.toml
COPY crates/ipvs-ffi/Cargo.toml crates/ipvs-ffi/Cargo.toml
COPY crates/vrrp/Cargo.toml crates/vrrp/Cargo.toml
COPY crates/vrrp-ffi/Cargo.toml crates/vrrp-ffi/Cargo.toml

# Create stub source files so cargo can resolve the workspace and cache deps
RUN for crate in common healthcheck healthcheck-ffi ipvs ipvs-ffi vrrp vrrp-ffi; do \
      mkdir -p crates/$crate/src && echo "" > crates/$crate/src/lib.rs; \
    done && \
    mkdir -p crates/healthcheck-server/src && \
    echo "" > crates/healthcheck-server/src/lib.rs && \
    echo "fn main() {}" > crates/healthcheck-server/src/main.rs

# Cache dependency compilation (ignore errors from stubs)
RUN cargo build --release -p healthcheck-server 2>/dev/null || true

# Copy actual source code
COPY crates/ crates/

# Invalidate stubs and build for real
RUN touch crates/common/src/lib.rs \
          crates/healthcheck/src/lib.rs \
          crates/healthcheck-server/src/lib.rs \
          crates/healthcheck-server/src/main.rs && \
    cargo build --release -p healthcheck-server

# Stage 2: Runtime
FROM debian:trixie-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/healthcheck-server /usr/local/bin/healthcheck-server

RUN mkdir -p /var/run/seesaw

WORKDIR /app

ENTRYPOINT ["healthcheck-server"]
