# ClawdFlow MCP Server Dockerfile
# Build from parent directory containing flowr-system/ and AQL/:
#   docker build -f flowr-system/flowr-ce/Dockerfile .
# Or from flowr-system/ with AQL alongside:
#   docker build -f flowr-ce/Dockerfile --build-context aql=../AQL .

# Build stage
FROM rust:1.88-bookworm AS builder

WORKDIR /build

# Install build dependencies for LanceDB/Arrow + git for ZeroClaw
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
# Cargo.toml path: ../../AQL from /build/flowr-ce/ resolves to /AQL
COPY AQL /AQL

# Copy clawdflow workspace
COPY flowr-ce/Cargo.toml flowr-ce/Cargo.lock /build/flowr-ce/
COPY flowr-ce/crates /build/flowr-ce/crates

WORKDIR /build/flowr-ce

# Build release binary (FlowR-CE runtime; crate renamed flowr-ce -> flowr-ce)
RUN cargo build --release -p flowr-ce

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create data directory
RUN mkdir -p /data/clawdflow

# Copy binary from builder (bin renamed clawdflow -> flowr-ce)
COPY --from=builder /build/flowr-ce/target/release/flowr-ce /usr/local/bin/flowr-ce

# Set environment variables
ENV CLAWDFLOW_DATA_DIR=/data/clawdflow
ENV RUST_LOG=info

# Run as non-root user
RUN useradd -m -s /bin/bash clawdflow
RUN chown -R clawdflow:clawdflow /data/clawdflow
USER clawdflow

# MCP uses stdio, so we need interactive mode
ENTRYPOINT ["/usr/local/bin/flowr-ce"]
