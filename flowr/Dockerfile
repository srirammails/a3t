# FlowR EE Server Dockerfile (CE + Auth + RBAC + UI)
# Build from flowr-system/:
#   docker build -f flowr/Dockerfile -t ghcr.io/srirammails/flowr:latest .

# Build stage
# Rust 1.89+ required by cedar-policy 4.11 (the EE Cedar engine).
FROM rust:1.89-bookworm AS builder

WORKDIR /build

# Install build dependencies for LanceDB/Arrow + git
RUN apt-get update && apt-get install -y \
    cmake \
    protobuf-compiler \
    libssl-dev \
    pkg-config \
    clang \
    git \
    && rm -rf /var/lib/apt/lists/*

# Copy shared crates first (for layer caching)
COPY shared /build/shared

# Copy proto files (for gRPC)
COPY proto /build/proto

# Copy AQL/ClawDB crates (workspace dependency)
COPY AQL /AQL

# Copy the AESOP decision core (EE Cedar engine). flowr-aesop/aesop-tower resolve
# it via `../../../../aesop/crates/*`, which lands at /aesop in the build image.
COPY aesop /aesop

# Copy clawdflow crates (flowr-mcp depends on flowr-ce-storage → flowr-ce-agents → flowr-ce-core)
COPY flowr-ce/Cargo.toml flowr-ce/Cargo.lock /build/flowr-ce/
COPY flowr-ce/crates/flowr-ce-storage /build/flowr-ce/crates/flowr-ce-storage
COPY flowr-ce/crates/flowr-ce-agents /build/flowr-ce/crates/flowr-ce-agents
COPY flowr-ce/crates/flowr-ce-core /build/flowr-ce/crates/flowr-ce-core
COPY flowr-ce/crates/flowr-ce /build/flowr-ce/crates/flowr-ce

# Copy flowr workspace
COPY flowr/Cargo.toml flowr/Cargo.lock /build/flowr/
COPY flowr/crates /build/flowr/crates

WORKDIR /build/flowr

# Build release binary (EE server — CE + auth + RBAC + Cedar policy hook + OIDC).
# `--features ee` pulls in flowr-aesop + aesop-tower (the Cedar enforcement path).
RUN cargo build --release -p flowr-server --features ee

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create data directories
RUN mkdir -p /data/flowr /data/a3t /config

# Copy binary from builder
COPY --from=builder /build/flowr/target/release/flowr /usr/local/bin/flowr

# Copy EE config files (RBAC model + policy)
COPY --from=builder /build/flowr/crates/flowr-server/config/ /config/

# Set environment variables
ENV RUST_LOG=info

# Run as non-root user
RUN useradd -m -s /bin/bash flowr
RUN chown -R flowr:flowr /data/flowr /data/a3t
USER flowr

ENTRYPOINT ["/usr/local/bin/flowr"]
